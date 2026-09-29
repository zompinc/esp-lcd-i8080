# esp-lcd-i8080

Fast parallel displays for ESP32-S3 in Rust, plus board support for the LILYGO T-Display-S3.

| Crate | What it is |
|---|---|
| [`esp-lcd-i8080`](esp-lcd-i8080) | A [`mipidsi`](https://crates.io/crates/mipidsi) display interface for the ESP32-S3 LCD_CAM peripheral in 8-bit i8080 mode, with DMA double buffering. Works with any MIPI DCS controller `mipidsi` supports. |
| [`lilygo-t-display-s3`](lilygo-t-display-s3) | Board support for the LILYGO T-Display-S3: display, backlight, buttons, battery and a frame buffer for animation. |

## Building

Both crates target `xtensa-esp32s3-none-elf`. Install the Xtensa Rust toolchain with
[`espup`](https://github.com/esp-rs/espup) and [`espflash`](https://github.com/esp-rs/espflash):

```sh
cargo install espup espflash --locked
espup install --targets esp32s3
```

Then, with a T-Display-S3 on USB:

```sh
cargo run --release --example hello
cargo run --release --example demo
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
