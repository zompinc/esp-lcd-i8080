#![no_std]
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

use esp_hal::Blocking;
use esp_hal::dma::{DmaError, DmaTxBuf};
use esp_hal::lcd_cam::lcd::i8080::{Command, I8080, I8080Transfer};
use mipidsi::interface::{Interface, InterfaceKind};

/// MIPI DCS `write_memory_start`.
const WRITE_MEMORY_START: u8 = 0x2C;
/// MIPI DCS `write_memory_continue`.
const WRITE_MEMORY_CONTINUE: u8 = 0x3C;

/// Sent as the only parameter of parameterless commands.
///
/// esp-hal's i8080 driver cannot start a DMA transfer without data, so a command such as
/// `SLPOUT` goes out followed by this byte. MIPI DCS controllers ignore surplus parameter bytes.
const FILLER: u8 = 0x00;

/// Errors reported by [`I8080Interface`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A DMA transfer failed to start or complete.
    Dma(DmaError),
    /// A DMA buffer is too small to hold a single command's parameters or one pixel.
    BufferTooSmall,
}

impl From<DmaError> for Error {
    fn from(err: DmaError) -> Self {
        Error::Dma(err)
    }
}

enum Bus<'d> {
    Idle(I8080<'d, Blocking>),
    Busy(I8080Transfer<'d, DmaTxBuf, Blocking>),
    // Only observable if a panic unwinds mid-transfer; kept so the state can be moved out.
    Poisoned,
}

/// A [`mipidsi`] display interface over the ESP32-S3 LCD_CAM peripheral in 8-bit i8080 mode.
///
/// Pixels are copied into DMA buffers and streamed to the panel by the LCD_CAM hardware. With two
/// buffers, the CPU fills one while the other is on the wire, and the last transfer of a frame is
/// left running when [`Interface::send_pixels`] returns, so the caller can start rendering the next
/// frame immediately. Every other call waits for that transfer first; [`I8080Interface::flush`]
/// waits explicitly.
///
/// # Chip select
///
/// Tie CS low (or drive it low from a GPIO) instead of handing it to the i8080 driver with
/// `with_cs`. A frame is sent as several DMA transfers, and toggling CS between them is not needed.
pub struct I8080Interface<'d> {
    bus: Bus<'d>,
    free: [Option<DmaTxBuf>; 2],
    /// `write_memory_start` / `write_memory_continue` held back until pixels arrive.
    pending_write: Option<u8>,
}

impl<'d> I8080Interface<'d> {
    /// Creates an interface that uses one DMA buffer.
    ///
    /// The CPU waits for each chunk to finish before filling the buffer again. Prefer
    /// [`I8080Interface::new_double_buffered`] when memory allows.
    pub fn new(bus: I8080<'d, Blocking>, buffer: DmaTxBuf) -> Self {
        Self {
            bus: Bus::Idle(bus),
            free: [Some(buffer), None],
            pending_write: None,
        }
    }

    /// Creates an interface that fills one DMA buffer while the other is being sent.
    ///
    /// Buffers of 4-32 KB each work well; larger buffers mean fewer, longer transfers.
    pub fn new_double_buffered(
        bus: I8080<'d, Blocking>,
        first: DmaTxBuf,
        second: DmaTxBuf,
    ) -> Self {
        Self {
            bus: Bus::Idle(bus),
            free: [Some(first), Some(second)],
            pending_write: None,
        }
    }

    /// Waits for any transfer still in flight.
    pub fn flush(&mut self) -> Result<(), Error> {
        match core::mem::replace(&mut self.bus, Bus::Poisoned) {
            Bus::Busy(transfer) => {
                let (result, bus, buffer) = transfer.wait();
                self.bus = Bus::Idle(bus);
                self.put_free(buffer);
                result.map_err(Error::from)
            }
            idle => {
                self.bus = idle;
                Ok(())
            }
        }
    }

    /// Waits for any transfer in flight and returns the driver and buffers.
    pub fn release(mut self) -> (I8080<'d, Blocking>, DmaTxBuf, Option<DmaTxBuf>) {
        // A failed final transfer still hands the bus and buffer back.
        self.flush().ok();
        let Bus::Idle(bus) = core::mem::replace(&mut self.bus, Bus::Poisoned) else {
            unreachable!("flush leaves the bus idle");
        };
        let [first, second] = &mut self.free;
        let first = first
            .take()
            .or_else(|| second.take())
            .expect("at least one buffer");
        (bus, first, second.take())
    }

    fn put_free(&mut self, buffer: DmaTxBuf) {
        let slot = self
            .free
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("a free slot");
        *slot = Some(buffer);
    }

    /// Returns a buffer the CPU may write to, waiting for the transfer in flight if needed.
    fn take_free(&mut self) -> Result<DmaTxBuf, Error> {
        if let Some(buffer) = self.free.iter_mut().find_map(Option::take) {
            return Ok(buffer);
        }
        self.flush()?;
        Ok(self
            .free
            .iter_mut()
            .find_map(Option::take)
            .expect("flush returns the buffer"))
    }

    /// Starts sending `len` bytes of `buffer`, after the previous transfer finishes.
    fn submit(
        &mut self,
        command: Command<u8>,
        mut buffer: DmaTxBuf,
        len: usize,
    ) -> Result<(), Error> {
        if let Err(err) = self.flush() {
            self.put_free(buffer);
            return Err(err);
        }
        let Bus::Idle(bus) = core::mem::replace(&mut self.bus, Bus::Poisoned) else {
            unreachable!("flush leaves the bus idle");
        };
        buffer.set_length(len);
        match bus.send(command, 0, buffer) {
            Ok(transfer) => {
                self.bus = Bus::Busy(transfer);
                Ok(())
            }
            Err((err, bus, buffer)) => {
                self.bus = Bus::Idle(bus);
                self.put_free(buffer);
                Err(err.into())
            }
        }
    }

    /// The command for the next data transfer: a held-back memory write, or none to continue one.
    fn next_command(&mut self) -> Command<u8> {
        self.pending_write
            .take()
            .map_or(Command::None, Command::One)
    }
}

impl Interface for I8080Interface<'_> {
    type Word = u8;
    type Error = Error;

    const KIND: InterfaceKind = InterfaceKind::Parallel8Bit;

    fn send_command(&mut self, command: u8, args: &[u8]) -> Result<(), Self::Error> {
        // A memory write with nothing written after it has no effect, so a stale one is dropped.
        self.pending_write = None;

        if args.is_empty() && matches!(command, WRITE_MEMORY_START | WRITE_MEMORY_CONTINUE) {
            // Sent together with the first pixel chunk; a filler byte here would land in GRAM.
            self.pending_write = Some(command);
            return self.flush();
        }

        let args = if args.is_empty() { &[FILLER][..] } else { args };
        let mut buffer = self.take_free()?;
        if args.len() > buffer.capacity() {
            self.put_free(buffer);
            return Err(Error::BufferTooSmall);
        }
        buffer.as_mut_slice()[..args.len()].copy_from_slice(args);
        self.submit(Command::One(command), buffer, args.len())?;
        // Commands are tiny; finishing them here keeps delays that follow (e.g. after SWRESET) exact.
        self.flush()
    }

    fn send_pixels<const N: usize>(
        &mut self,
        pixels: impl IntoIterator<Item = [Self::Word; N]>,
    ) -> Result<(), Self::Error> {
        let mut pixels = pixels.into_iter().peekable();
        while pixels.peek().is_some() {
            let mut buffer = self.take_free()?;
            let slice = buffer.as_mut_slice();
            let usable = slice.len() / N * N;
            if usable == 0 {
                self.put_free(buffer);
                return Err(Error::BufferTooSmall);
            }

            let mut len = 0;
            for (chunk, pixel) in slice[..usable].chunks_exact_mut(N).zip(pixels.by_ref()) {
                chunk.copy_from_slice(&pixel);
                len += N;
            }
            let command = self.next_command();
            self.submit(command, buffer, len)?;
        }
        Ok(())
    }

    fn send_repeated_pixel<const N: usize>(
        &mut self,
        pixel: [Self::Word; N],
        count: u32,
    ) -> Result<(), Self::Error> {
        let mut remaining = count as usize;
        while remaining > 0 {
            let mut buffer = self.take_free()?;
            let slice = buffer.as_mut_slice();
            let n = remaining.min(slice.len() / N);
            if n == 0 {
                self.put_free(buffer);
                return Err(Error::BufferTooSmall);
            }
            for chunk in slice[..n * N].chunks_exact_mut(N) {
                chunk.copy_from_slice(&pixel);
            }
            let command = self.next_command();
            self.submit(command, buffer, n * N)?;
            remaining -= n;
        }
        Ok(())
    }
}
