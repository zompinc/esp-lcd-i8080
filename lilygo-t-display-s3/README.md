# lilygo-t-display-s3

Board support for the [LILYGO T-Display-S3](https://github.com/Xinyuan-LilyGO/T-Display-S3)
(ESP32-S3, 1.9" 170x320 ST7789 on an 8-bit parallel bus), built on
[`esp-hal`](https://crates.io/crates/esp-hal) and [`mipidsi`](https://crates.io/crates/mipidsi).

One call gives you:

- **Display**: a `mipidsi` ST7789 in landscape (320x170), driven over DMA by
  [`esp-lcd-i8080`](https://crates.io/crates/esp-lcd-i8080), with the panel offset, color
  inversion and battery power pin already handled.
- **Backlight**: PWM brightness from 0 to 100%.
- **Buttons**: BOOT (GPIO0) and KEY (GPIO14) with pull-ups.
- **Battery**: calibrated voltage in millivolts.
- **`FrameBuffer`**: an `embedded-graphics` draw target that sends a whole frame at once, for
  flicker-free animation.

```rust,ignore
use embedded_graphics::{mono_font::{MonoTextStyle, ascii::FONT_10X20}, pixelcolor::Rgb565, prelude::*, text::Text};
use lilygo_t_display_s3::{Board, resources};

let peripherals = esp_hal::init(esp_hal::Config::default());
let mut board = Board::new(resources!(peripherals)).unwrap();

board.display.clear(Rgb565::BLACK).unwrap();
Text::new("Hello, T-Display-S3!", Point::new(20, 90), MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE))
    .draw(&mut board.display)
    .unwrap();
```

## Examples

Flash with the board on USB (install the Xtensa toolchain with `espup install` first):

```sh
cargo run --release --example hello
cargo run --release --example demo
```

- `hello`: text, shapes and the battery voltage, updated when a button is pressed.
- `demo`: a greeting with confetti, a plasma animation, system info and a WiFi scan, cycled
  with BOOT; KEY changes brightness.

## Pin map

| Function | GPIO |
|---|---|
| LCD D0-D7 | 39, 40, 41, 42, 45, 46, 47, 48 |
| LCD WR / RD / DC / CS / RST | 8 / 9 / 7 / 6 / 5 |
| LCD power (needed on battery) | 15 |
| Backlight | 38 |
| Buttons BOOT / KEY | 0 / 14 |
| Battery ADC (halved by a divider) | 4 |

The touch variant's CST816 controller (I2C on 18/17, reset 21, interrupt 16) is not wired up by
this crate; the [`cst816s`](https://crates.io/crates/cst816s) crate drives it.

## License

Licensed under the [MIT license](https://github.com/zompinc/esp-lcd-i8080/blob/master/LICENSE).
