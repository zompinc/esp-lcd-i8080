# esp-lcd-i8080

A [`mipidsi`](https://crates.io/crates/mipidsi) display interface for the ESP32-S3 LCD_CAM
peripheral in 8-bit i8080 (8080-style parallel) mode, built on
[`esp-hal`](https://crates.io/crates/esp-hal).

`mipidsi` ships a parallel interface that toggles GPIOs one write at a time, which is too slow for
full-frame animation. This crate hands the bytes to the LCD_CAM hardware over DMA instead:

- **DMA double buffering**: the CPU fills one buffer while the other is on the wire.
- **Non-blocking frame tail**: the last chunk of a frame is still being sent when `send_pixels`
  returns, so rendering of the next frame overlaps with it.
- **Any MIPI DCS controller** that `mipidsi` supports on an 8-bit parallel bus (ST7789, ILI9341,
  ST7796 and others).

## Usage

Enable the chip feature and `esp-hal`'s `unstable` feature, which the i8080 driver requires:

```toml
[dependencies]
esp-lcd-i8080 = { version = "0.1", features = ["esp32s3"] }
esp-hal       = { version = "~1.2.2", features = ["esp32s3", "unstable"] }
mipidsi       = "0.10"
```

```rust,ignore
use esp_hal::{dma_tx_buffer, lcd_cam::{LcdCam, lcd::i8080::{Config, I8080}}, time::Rate};
use esp_lcd_i8080::I8080Interface;
use mipidsi::{Builder, models::ST7789};

let lcd_cam = LcdCam::new(peripherals.LCD_CAM);
let bus = I8080::new(lcd_cam.lcd, peripherals.DMA_CH0, Config::default().with_frequency(Rate::from_mhz(20)))?
    .with_dc(peripherals.GPIO7)
    .with_wrx(peripherals.GPIO8)
    .with_data0(peripherals.GPIO39)
    // ... data1 to data7
    ;

let interface = I8080Interface::new_double_buffered(
    bus,
    dma_tx_buffer!(16 * 1024)?,
    dma_tx_buffer!(16 * 1024)?,
);
let mut display = Builder::new(ST7789, interface)
    .display_size(170, 320)
    .init(&mut delay)?;
```

Keep chip select low (tie it or drive it from a GPIO) rather than passing it to `with_cs`: a frame
goes out as several DMA transfers.

## How commands are sent

The i8080 driver cannot run a DMA transfer without data, which affects commands that take no
parameters:

- `write_memory_start` and `write_memory_continue` are held back and sent as the command phase of
  the first pixel chunk. Every later chunk starts with `write_memory_continue`: continuing without
  a command phase leaves every other chunk byte-swapped on real hardware.
- Any other parameterless command (`SLPOUT`, `DISPON`, ...) is followed by one `0x00` byte, which
  MIPI DCS controllers ignore.

## Limitations

- 8-bit bus only; 16-bit i8080 is not supported yet.
- Blocking driver mode; an async variant is planned.
- ESP32-S3 only, since it is the only chip with an esp-hal i8080 driver.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
