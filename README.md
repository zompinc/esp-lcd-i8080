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

## Releasing

Bump the crate's `version` in its `Cargo.toml`, merge, then push a tag named after the crate:

```sh
git tag esp-lcd-i8080-v0.1.1
git push origin esp-lcd-i8080-v0.1.1
```

The `Release` workflow checks that the tag matches `Cargo.toml`, attests build provenance for the
packaged `.crate`, and publishes it with crates.io Trusted Publishing (no stored token). Verify a
release with `gh attestation verify <file>.crate --repo zompinc/esp-lcd-i8080`.

## License

Licensed under the [MIT license](LICENSE).
