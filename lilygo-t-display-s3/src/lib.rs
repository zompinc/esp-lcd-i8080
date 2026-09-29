#![no_std]
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

use core::convert::Infallible;
use core::sync::atomic::{AtomicBool, Ordering};

use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;
use embedded_graphics_core::primitives::Rectangle;
use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcConfig, AdcPin, Attenuation};
use esp_hal::delay::Delay;
use esp_hal::dma::DmaBufError;
use esp_hal::dma_tx_buffer;
use esp_hal::gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::lcd_cam::LcdCam;
use esp_hal::lcd_cam::lcd::i8080::{self, I8080};
use esp_hal::ledc::channel::{self, Channel, ChannelIFace};
use esp_hal::ledc::timer::{self, Timer, TimerIFace};
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::peripherals::{
    ADC1, DMA_CH0, GPIO0, GPIO4, GPIO5, GPIO6, GPIO7, GPIO8, GPIO9, GPIO14, GPIO15, GPIO38, GPIO39,
    GPIO40, GPIO41, GPIO42, GPIO45, GPIO46, GPIO47, GPIO48, LCD_CAM, LEDC,
};
use esp_hal::time::Rate;
pub use esp_lcd_i8080::{self, I8080Interface};
pub use mipidsi;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, Display};
use static_cell::StaticCell;

/// Display width in pixels, in the landscape orientation [`Board`] sets up.
pub const WIDTH: u16 = 320;
/// Display height in pixels, in the landscape orientation [`Board`] sets up.
pub const HEIGHT: u16 = 170;

/// Size of each of the two DMA staging buffers used for the display.
const DMA_CHUNK: usize = 16 * 1024;

/// The board's display: an ST7789 driven by [`mipidsi`] over [`I8080Interface`].
pub type Lcd = Display<I8080Interface<'static>, ST7789, Output<'static>>;

/// The peripherals and pins the board uses. Build it with [`resources!`].
#[allow(missing_docs)]
pub struct Resources {
    pub lcd_cam: LCD_CAM<'static>,
    pub dma: DMA_CH0<'static>,
    pub ledc: LEDC<'static>,
    pub adc1: ADC1<'static>,
    pub lcd_power: GPIO15<'static>,
    pub lcd_rd: GPIO9<'static>,
    pub lcd_cs: GPIO6<'static>,
    pub lcd_reset: GPIO5<'static>,
    pub lcd_dc: GPIO7<'static>,
    pub lcd_wr: GPIO8<'static>,
    pub lcd_d0: GPIO39<'static>,
    pub lcd_d1: GPIO40<'static>,
    pub lcd_d2: GPIO41<'static>,
    pub lcd_d3: GPIO42<'static>,
    pub lcd_d4: GPIO45<'static>,
    pub lcd_d5: GPIO46<'static>,
    pub lcd_d6: GPIO47<'static>,
    pub lcd_d7: GPIO48<'static>,
    pub backlight: GPIO38<'static>,
    pub button_boot: GPIO0<'static>,
    pub button_key: GPIO14<'static>,
    pub battery: GPIO4<'static>,
}

/// Moves the board's peripherals out of `esp_hal::init`'s result into a [`Resources`].
///
/// The rest of the peripherals (WiFi, timers, the free header pins) stay available.
///
/// ```rust,ignore
/// let peripherals = esp_hal::init(esp_hal::Config::default());
/// let board = lilygo_t_display_s3::Board::new(lilygo_t_display_s3::resources!(peripherals))?;
/// ```
#[macro_export]
macro_rules! resources {
    ($p:ident) => {
        $crate::Resources {
            lcd_cam: $p.LCD_CAM,
            dma: $p.DMA_CH0,
            ledc: $p.LEDC,
            adc1: $p.ADC1,
            lcd_power: $p.GPIO15,
            lcd_rd: $p.GPIO9,
            lcd_cs: $p.GPIO6,
            lcd_reset: $p.GPIO5,
            lcd_dc: $p.GPIO7,
            lcd_wr: $p.GPIO8,
            lcd_d0: $p.GPIO39,
            lcd_d1: $p.GPIO40,
            lcd_d2: $p.GPIO41,
            lcd_d3: $p.GPIO42,
            lcd_d4: $p.GPIO45,
            lcd_d5: $p.GPIO46,
            lcd_d6: $p.GPIO47,
            lcd_d7: $p.GPIO48,
            backlight: $p.GPIO38,
            button_boot: $p.GPIO0,
            button_key: $p.GPIO14,
            battery: $p.GPIO4,
        }
    };
}

/// Errors from [`Board::new`].
#[derive(Debug)]
pub enum Error {
    /// [`Board::new`] was called a second time.
    AlreadyInitialized,
    /// The i8080 bus rejected its configuration.
    Bus(i8080::ConfigError),
    /// A DMA staging buffer could not be set up.
    DmaBuffer(DmaBufError),
    /// Sending the display's init sequence failed.
    Display(esp_lcd_i8080::Error),
    /// mipidsi rejected the display configuration.
    DisplayConfig,
    /// The backlight PWM could not be configured.
    Backlight,
}

/// Everything on the T-Display-S3, set up and ready to use.
pub struct Board {
    /// The 1.9" ST7789 display in landscape orientation ([`WIDTH`] x [`HEIGHT`]).
    pub display: Lcd,
    /// Backlight brightness control.
    pub backlight: Backlight,
    /// The BOOT button (GPIO0), low while pressed.
    pub boot_button: Input<'static>,
    /// The KEY button (GPIO14), low while pressed.
    pub key_button: Input<'static>,
    /// Battery voltage.
    pub battery: Battery,
    // Held so the pins keep their levels for as long as the board exists.
    _lcd_power: Output<'static>,
    _lcd_rd: Output<'static>,
    _lcd_cs: Output<'static>,
}

impl Board {
    /// Powers up and initializes the display, backlight, buttons and battery ADC.
    ///
    /// Call once; the display's DMA buffers are statically allocated.
    pub fn new(r: Resources) -> Result<Self, Error> {
        static TAKEN: AtomicBool = AtomicBool::new(false);
        if TAKEN.swap(true, Ordering::AcqRel) {
            return Err(Error::AlreadyInitialized);
        }

        let out = OutputConfig::default();
        // The panel is powered from GPIO15 when running on battery.
        let lcd_power = Output::new(r.lcd_power, Level::High, out);
        let lcd_rd = Output::new(r.lcd_rd, Level::High, out);
        let lcd_cs = Output::new(r.lcd_cs, Level::Low, out);
        let lcd_reset = Output::new(r.lcd_reset, Level::High, out);

        let lcd_cam = LcdCam::new(r.lcd_cam);
        let bus = I8080::new(
            lcd_cam.lcd,
            r.dma,
            i8080::Config::default().with_frequency(Rate::from_mhz(20)),
        )
        .map_err(Error::Bus)?
        .with_dc(r.lcd_dc)
        .with_wrx(r.lcd_wr)
        .with_data0(r.lcd_d0)
        .with_data1(r.lcd_d1)
        .with_data2(r.lcd_d2)
        .with_data3(r.lcd_d3)
        .with_data4(r.lcd_d4)
        .with_data5(r.lcd_d5)
        .with_data6(r.lcd_d6)
        .with_data7(r.lcd_d7);

        let first = dma_tx_buffer!(DMA_CHUNK).map_err(Error::DmaBuffer)?;
        let second = dma_tx_buffer!(DMA_CHUNK).map_err(Error::DmaBuffer)?;
        let interface = I8080Interface::new_double_buffered(bus, first, second);

        let display = Builder::new(ST7789, interface)
            .display_size(HEIGHT, WIDTH)
            // The 170-pixel-wide glass sits in the middle of the controller's 240-pixel RAM.
            .display_offset(35, 0)
            .invert_colors(ColorInversion::Inverted)
            .orientation(Orientation::new().rotate(Rotation::Deg90))
            .reset_pin(lcd_reset)
            .init(&mut Delay::new())
            .map_err(|err| match err {
                mipidsi::InitError::Interface(err) => Error::Display(err),
                _ => Error::DisplayConfig,
            })?;

        let pull_up = InputConfig::default().with_pull(Pull::Up);

        Ok(Self {
            display,
            backlight: Backlight::new(r.ledc, r.backlight)?,
            boot_button: Input::new(r.button_boot, pull_up),
            key_button: Input::new(r.button_key, pull_up),
            battery: Battery::new(r.adc1, r.battery),
            _lcd_power: lcd_power,
            _lcd_rd: lcd_rd,
            _lcd_cs: lcd_cs,
        })
    }
}

/// PWM backlight on GPIO38.
pub struct Backlight {
    channel: Channel<'static, LowSpeed>,
    percent: u8,
}

impl Backlight {
    fn new(ledc: LEDC<'static>, pin: GPIO38<'static>) -> Result<Self, Error> {
        static TIMER: StaticCell<Timer<'static, LowSpeed>> = StaticCell::new();

        let mut ledc = Ledc::new(ledc);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
        let timer = TIMER.init(ledc.timer::<LowSpeed>(timer::Number::Timer0));
        timer
            .configure(timer::config::Config {
                duty: timer::config::Duty::Duty8Bit,
                clock_source: timer::LSClockSource::APBClk,
                frequency: Rate::from_khz(22),
            })
            .map_err(|_| Error::Backlight)?;

        let mut channel = ledc.channel(channel::Number::Channel0, pin);
        channel
            .configure(channel::config::Config {
                timer: &*timer,
                duty_pct: 100,
                drive_mode: DriveMode::PushPull,
            })
            .map_err(|_| Error::Backlight)?;

        Ok(Self {
            channel,
            percent: 100,
        })
    }

    /// Sets brightness from 0 (off) to 100 (full).
    pub fn set_percent(&mut self, percent: u8) {
        let percent = percent.min(100);
        // Only fails for values above 100, which are clamped above.
        self.channel.set_duty(percent).ok();
        self.percent = percent;
    }

    /// Current brightness from 0 to 100.
    pub fn percent(&self) -> u8 {
        self.percent
    }
}

/// Battery voltage, read through the board's divider on GPIO4.
pub struct Battery {
    adc: Adc<'static, ADC1<'static>, Blocking>,
    pin: AdcPin<GPIO4<'static>, ADC1<'static>, AdcCalCurve<ADC1<'static>>>,
}

impl Battery {
    fn new(adc1: ADC1<'static>, pin: GPIO4<'static>) -> Self {
        let mut config = AdcConfig::new();
        let pin =
            config.enable_pin_with_cal::<_, AdcCalCurve<ADC1<'static>>>(pin, Attenuation::_11dB);
        Self {
            adc: Adc::new(adc1, config),
            pin,
        }
    }

    /// Voltage at the battery connector in millivolts.
    ///
    /// Reads about 5000 mV on USB power with no cell attached.
    pub fn millivolts(&mut self) -> u32 {
        // The divider halves the cell voltage before the ADC.
        u32::from(self.adc.read_blocking(&mut self.pin)) * 2
    }
}

/// A full-screen RGB565 frame to draw into with `embedded-graphics`, then send in one go.
///
/// Drawing straight to [`Lcd`] sends every primitive separately; for animation, draw a frame here
/// and call [`FrameBuffer::flush`]. The pixels need `WIDTH * HEIGHT` entries (about 106 KB), for
/// example from PSRAM: `Vec::leak(vec![Rgb565::BLACK; FrameBuffer::LEN])`.
pub struct FrameBuffer<'a> {
    pixels: &'a mut [Rgb565],
}

impl<'a> FrameBuffer<'a> {
    /// Number of pixels in a frame.
    pub const LEN: usize = WIDTH as usize * HEIGHT as usize;

    /// Wraps `pixels`, which must hold exactly [`FrameBuffer::LEN`] entries in row-major order.
    pub fn new(pixels: &'a mut [Rgb565]) -> Self {
        assert_eq!(
            pixels.len(),
            Self::LEN,
            "frame buffer must hold WIDTH * HEIGHT pixels"
        );
        Self { pixels }
    }

    /// Direct access to the pixels in row-major order, for effects that write every pixel.
    pub fn pixels_mut(&mut self) -> &mut [Rgb565] {
        self.pixels
    }

    /// Sends the whole frame to the display.
    ///
    /// Returns while the final DMA chunk is still being sent, so drawing the next frame can start.
    pub fn flush(&self, display: &mut Lcd) -> Result<(), esp_lcd_i8080::Error> {
        display.set_pixels(0, 0, WIDTH - 1, HEIGHT - 1, self.pixels.iter().copied())
    }
}

impl OriginDimensions for FrameBuffer<'_> {
    fn size(&self) -> Size {
        Size::new(u32::from(WIDTH), u32::from(HEIGHT))
    }
}

impl DrawTarget for FrameBuffer<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let (w, h) = (i32::from(WIDTH), i32::from(HEIGHT));
        for Pixel(p, color) in pixels {
            if (0..w).contains(&p.x) && (0..h).contains(&p.y) {
                self.pixels[(p.y * w + p.x) as usize] = color;
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let area = area.intersection(&self.bounding_box());
        let Some(bottom_right) = area.bottom_right() else {
            return Ok(());
        };
        let w = WIDTH as usize;
        for y in area.top_left.y as usize..=bottom_right.y as usize {
            self.pixels[y * w + area.top_left.x as usize..=y * w + bottom_right.x as usize]
                .fill(color);
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.pixels.fill(color);
        Ok(())
    }
}
