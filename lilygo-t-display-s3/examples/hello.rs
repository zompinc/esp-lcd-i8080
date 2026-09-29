//! Text, shapes and the battery voltage; press either button to redraw with a new count.

#![no_std]
#![no_main]

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::{Rgb565, WebColors};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle, Rectangle, RoundedRectangle};
use embedded_graphics::text::{Alignment, Text};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::main;
use lilygo_t_display_s3::{Board, HEIGHT, Lcd, WIDTH, resources};
use log::info;

esp_bootloader_esp_idf::esp_app_desc!();

fn draw(display: &mut Lcd, presses: u32, battery_mv: u32) -> Result<(), esp_lcd_i8080::Error> {
    display.clear(Rgb565::CSS_MIDNIGHT_BLUE)?;

    let card = Rectangle::new(
        Point::new(10, 10),
        Size::new(u32::from(WIDTH) - 20, u32::from(HEIGHT) - 20),
    );
    RoundedRectangle::with_equal_corners(card, Size::new(12, 12))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_GOLD, 2))
        .draw(display)?;

    let center = Point::new(i32::from(WIDTH) / 2, 0);
    Text::with_alignment(
        "Hello, T-Display-S3!",
        center + Point::new(0, 55),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
        Alignment::Center,
    )
    .draw(display)?;

    let mut line = heapless::String::<48>::new();
    write!(line, "Button presses: {presses}   Battery: {battery_mv} mV").ok();
    Text::with_alignment(
        &line,
        center + Point::new(0, 90),
        MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_LIGHT_GRAY),
        Alignment::Center,
    )
    .draw(display)?;

    for (i, color) in [Rgb565::CSS_RED, Rgb565::CSS_LIME, Rgb565::CSS_DODGER_BLUE]
        .into_iter()
        .enumerate()
    {
        Circle::with_center(center + Point::new(-40 + 40 * i as i32, 125), 24)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)?;
    }
    Ok(())
}

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut board = Board::new(resources!(peripherals)).expect("board init");
    let delay = Delay::new();

    let mut presses = 0;
    let mut was_pressed = false;
    let mut redraw = true;
    loop {
        let pressed = board.boot_button.is_low() || board.key_button.is_low();
        if pressed && !was_pressed {
            presses += 1;
            redraw = true;
        }
        was_pressed = pressed;

        if redraw {
            let battery_mv = board.battery.millivolts();
            draw(&mut board.display, presses, battery_mv).expect("draw");
            info!("presses {presses}, battery {battery_mv} mV");
            redraw = false;
        }
        // Also debounces the buttons.
        delay.delay_millis(20);
    }
}
