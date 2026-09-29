//! The four demo pages, drawn into a [`FrameBuffer`] once per loop iteration.

use core::fmt::Write;

use embedded_graphics::pixelcolor::{Rgb565, WebColors};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use heapless::String;
use libm::{fabsf, sinf};
use u8g2_fonts::FontRenderer;
use u8g2_fonts::fonts;
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use lilygo_t_display_s3::{FrameBuffer, HEIGHT, WIDTH};

type Frame<'a> = FrameBuffer<'a>;

const W: i32 = WIDTH as i32;
const H: i32 = HEIGHT as i32;

const TITLE: FontRenderer = FontRenderer::new::<fonts::u8g2_font_helvB12_tf>();
const HUGE: FontRenderer = FontRenderer::new::<fonts::u8g2_font_helvB24_tf>();
const LARGE: FontRenderer = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
const BODY: FontRenderer = FontRenderer::new::<fonts::u8g2_font_helvR10_tf>();
const SMALL: FontRenderer = FontRenderer::new::<fonts::u8g2_font_helvR08_tf>();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Greeting,
    Plasma,
    Info,
    Wifi,
}

impl Page {
    pub fn next(self) -> Self {
        match self {
            Page::Greeting => Page::Plasma,
            Page::Plasma => Page::Info,
            Page::Info => Page::Wifi,
            Page::Wifi => Page::Greeting,
        }
    }
}

pub struct SystemInfo {
    pub chip_revision: (u8, u8),
    pub cpu_mhz: u32,
    pub heap_free: usize,
    pub heap_size: usize,
    pub psram_free: usize,
    pub psram_size: usize,
    pub mac: [u8; 6],
    pub uptime_secs: u64,
    pub battery_mv: u32,
    pub build: &'static str,
    pub fps: f32,
}

#[derive(Clone)]
pub struct AccessPoint {
    pub ssid: String<32>,
    pub rssi: i8,
    pub channel: u8,
}

pub struct WifiView<'a> {
    pub scanned: bool,
    pub networks: &'a [AccessPoint],
}

#[derive(Clone, Copy, Default)]
struct Confetti {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    phase: f32,
    color: Rgb565,
}

const CONFETTI_COUNT: usize = 70;
const CONFETTI_COLORS: [Rgb565; 7] = [
    Rgb565::CSS_RED,
    Rgb565::CSS_GOLD,
    Rgb565::CSS_LIME,
    Rgb565::CSS_CYAN,
    Rgb565::CSS_MAGENTA,
    Rgb565::CSS_ORANGE,
    Rgb565::WHITE,
];

pub struct App {
    pub page: Page,
    frame: u32,
    sine: [i8; 256],
    palette: [Rgb565; 256],
    confetti: [Confetti; CONFETTI_COUNT],
    rng: u32,
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

impl App {
    pub fn new(seed: u32) -> Self {
        let mut sine = [0i8; 256];
        let mut palette = [Rgb565::BLACK; 256];
        for i in 0..256 {
            let t = i as f32 * 2.0 * core::f32::consts::PI / 256.0;
            sine[i] = (127.0 * sinf(t)) as i8;
            let channel = |shift: f32| (128.0 + 127.0 * sinf(t + shift)) as u8;
            palette[i] = rgb(
                channel(0.0),
                channel(2.0 * core::f32::consts::PI / 3.0),
                channel(4.0 * core::f32::consts::PI / 3.0),
            );
        }

        let mut app = Self {
            page: Page::Greeting,
            frame: 0,
            sine,
            palette,
            confetti: [Confetti::default(); CONFETTI_COUNT],
            rng: seed | 1,
        };
        for i in 0..CONFETTI_COUNT {
            app.confetti[i] = app.new_confetti(true);
        }
        app
    }

    pub fn render(&mut self, f: &mut Frame, info: &SystemInfo, wifi: &WifiView) {
        match self.page {
            Page::Greeting => self.greeting(f),
            Page::Plasma => self.plasma(f, info.fps),
            Page::Info => info_page(f, info),
            Page::Wifi => wifi_page(f, wifi),
        }
        self.frame = self.frame.wrapping_add(1);
    }

    fn random(&mut self) -> u32 {
        // xorshift32
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn random_range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.random() % (hi - lo) as u32) as i32
    }

    fn new_confetti(&mut self, anywhere: bool) -> Confetti {
        Confetti {
            x: self.random_range(0, W) as f32,
            y: if anywhere {
                self.random_range(0, H)
            } else {
                -self.random_range(10, 40)
            } as f32,
            vx: self.random_range(-40, 41) as f32 / 100.0,
            vy: self.random_range(60, 180) as f32 / 100.0,
            phase: self.random_range(0, 628) as f32 / 100.0,
            color: CONFETTI_COLORS[self.random_range(0, CONFETTI_COLORS.len() as i32) as usize],
        }
    }

    fn greeting(&mut self, f: &mut Frame) {
        let t = self.frame as f32;

        // Deep blue to purple vertical gradient.
        for y in 0..H {
            let color = rgb(20 + (y / 3) as u8, 10, 60 + (y / 2) as u8);
            f.fill_solid(
                &Rectangle::new(Point::new(0, y), Size::new(W as u32, 1)),
                color,
            )
            .ok();
        }

        for i in 0..CONFETTI_COUNT {
            let mut c = self.confetti[i];
            c.x += c.vx + 0.4 * sinf(c.phase + t * 0.05);
            c.y += c.vy;
            // Width oscillates so each piece looks like it is tumbling.
            let w = 1 + (3.0 * fabsf(sinf(c.phase + t * 0.15))) as u32;
            Rectangle::new(Point::new(c.x as i32, c.y as i32), Size::new(w, 4))
                .into_styled(PrimitiveStyle::with_fill(c.color))
                .draw(f)
                .ok();
            if c.y > H as f32 || c.x < -5.0 || c.x > (W + 5) as f32 {
                c = self.new_confetti(false);
            }
            self.confetti[i] = c;
        }

        // Gentle bob for the name.
        let bob = (4.0 * sinf(t * 0.06)) as i32;
        let centered =
            |font: &FontRenderer, f: &mut Frame, text: &str, x: i32, y: i32, color: Rgb565| {
                font.render_aligned(
                    text,
                    Point::new(x, y),
                    VerticalPosition::Center,
                    HorizontalAlignment::Center,
                    FontColor::Transparent(color),
                    f,
                )
                .ok();
            };

        centered(&TITLE, f, "Hello,", W / 2, 38, Rgb565::CSS_GOLD);
        centered(
            &HUGE,
            f,
            "Travis Lu",
            W / 2 + 3,
            88 + bob + 3,
            Rgb565::BLACK,
        );
        let rainbow = self.palette[(self.frame.wrapping_mul(2) & 0xFF) as usize];
        centered(&HUGE, f, "Travis Lu", W / 2, 88 + bob, rainbow);
        centered(
            &BODY,
            f,
            "Great to have you here!",
            W / 2,
            140,
            Rgb565::WHITE,
        );
    }

    fn plasma(&mut self, f: &mut Frame, fps: f32) {
        let t = self.frame as i32;
        let sine = &self.sine;
        let palette = &self.palette;
        let s = |i: i32| sine[(i & 0xFF) as usize] as i32;
        let pixels = f.pixels_mut();
        for y in 0..H {
            let sy = s(y * 3 + t * 2);
            let row = &mut pixels[y as usize * W as usize..][..W as usize];
            for (x, px) in (0..W).zip(row) {
                let v = s(x * 2 + t) + sy + s(x + y + t * 3) + s((x - y) * 2 - t);
                *px = palette[(((v >> 2) + t) & 0xFF) as usize];
            }
        }

        for (dx, color) in [(2, Rgb565::BLACK), (0, Rgb565::WHITE)] {
            LARGE
                .render_aligned(
                    "T-Display-S3",
                    Point::new(W / 2 + dx, H / 2 + dx),
                    VerticalPosition::Center,
                    HorizontalAlignment::Center,
                    FontColor::Transparent(color),
                    f,
                )
                .ok();
        }

        let mut text: String<16> = String::new();
        write!(text, "{fps:.1} fps").ok();
        SMALL
            .render_aligned(
                text.as_str(),
                Point::new(W - 3, H - 3),
                VerticalPosition::Bottom,
                HorizontalAlignment::Right,
                FontColor::WithBackground {
                    fg: Rgb565::WHITE,
                    bg: Rgb565::BLACK,
                },
                f,
            )
            .ok();
    }
}

fn header(f: &mut Frame, title: &str) {
    f.clear(Rgb565::BLACK).ok();
    f.fill_solid(
        &Rectangle::new(Point::zero(), Size::new(W as u32, 22)),
        Rgb565::CSS_NAVY,
    )
    .ok();
    TITLE
        .render_aligned(
            title,
            Point::new(6, 11),
            VerticalPosition::Center,
            HorizontalAlignment::Left,
            FontColor::Transparent(Rgb565::WHITE),
            f,
        )
        .ok();
    SMALL
        .render_aligned(
            "BOOT: next  KEY: light",
            Point::new(W - 4, 11),
            VerticalPosition::Center,
            HorizontalAlignment::Right,
            FontColor::Transparent(Rgb565::WHITE),
            f,
        )
        .ok();
}

fn text(f: &mut Frame, font: &FontRenderer, s: &str, x: i32, y: i32, color: Rgb565) {
    font.render(
        s,
        Point::new(x, y),
        VerticalPosition::Top,
        FontColor::Transparent(color),
        f,
    )
    .ok();
}

fn info_page(f: &mut Frame, info: &SystemInfo) {
    header(f, "System");

    let mut y = 28;
    let mut row = |label: &str, value: &str, color: Rgb565| {
        text(f, &BODY, label, 8, y, Rgb565::CSS_GRAY);
        text(f, &BODY, value, 100, y, color);
        y += 17;
    };
    let mut s: String<64> = String::new();

    let (major, minor) = info.chip_revision;
    write!(
        s,
        "ESP32-S3 v{major}.{minor}, 2 cores @ {} MHz",
        info.cpu_mhz
    )
    .ok();
    row("Chip", &s, Rgb565::WHITE);

    s.clear();
    if info.psram_size > 0 {
        write!(
            s,
            "{} / {} KB free",
            info.psram_free / 1024,
            info.psram_size / 1024
        )
        .ok();
    } else {
        s.push_str("not detected").ok();
    }
    row("PSRAM", &s, Rgb565::WHITE);

    s.clear();
    write!(
        s,
        "{} / {} KB free",
        info.heap_free / 1024,
        info.heap_size / 1024
    )
    .ok();
    row("Heap", &s, Rgb565::WHITE);

    s.clear();
    let m = info.mac;
    write!(
        s,
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        m[0], m[1], m[2], m[3], m[4], m[5]
    )
    .ok();
    row("MAC", &s, Rgb565::WHITE);

    s.clear();
    let up = info.uptime_secs;
    write!(s, "{}h {}m {}s", up / 3600, (up / 60) % 60, up % 60).ok();
    row("Uptime", &s, Rgb565::WHITE);

    s.clear();
    let volts = info.battery_mv as f32 / 1000.0;
    // ~5 V on the divider means USB power with no cell attached.
    if volts > 4.5 {
        write!(s, "USB ({volts:.2} V)").ok();
    } else {
        write!(s, "{volts:.2} V").ok();
    }
    row(
        "Battery",
        &s,
        if volts > 3.6 {
            Rgb565::CSS_LIME
        } else {
            Rgb565::CSS_ORANGE
        },
    );

    row("Build", info.build, Rgb565::CSS_CYAN);
    row("Language", "Rust (esp-hal, no_std)", Rgb565::CSS_ORANGE);
}

fn wifi_page(f: &mut Frame, wifi: &WifiView) {
    header(f, "WiFi scan");

    if !wifi.scanned {
        text(f, &BODY, "Scanning...", 8, 30, Rgb565::YELLOW);
        return;
    }
    if wifi.networks.is_empty() {
        text(f, &BODY, "No networks found", 8, 30, Rgb565::CSS_ORANGE);
        return;
    }

    let mut y = 28;
    for ap in wifi.networks {
        if y >= H - 14 {
            break;
        }
        let bars = ((ap.rssi as i32 + 100) / 12).clamp(0, 5);
        for b in 0..5 {
            let bh = 3 + b * 2;
            let color = if b < bars {
                Rgb565::CSS_LIME
            } else {
                Rgb565::CSS_DIM_GRAY
            };
            f.fill_solid(
                &Rectangle::new(Point::new(8 + b * 4, y + 13 - bh), Size::new(3, bh as u32)),
                color,
            )
            .ok();
        }

        let ssid = if ap.ssid.is_empty() {
            "(hidden)"
        } else {
            ap.ssid.as_str()
        };
        let ssid = ssid
            .char_indices()
            .nth(26)
            .map_or(ssid, |(i, _)| &ssid[..i]);
        text(f, &BODY, ssid, 34, y, Rgb565::WHITE);

        let mut s: String<24> = String::new();
        write!(s, "{} dBm  ch{}", ap.rssi, ap.channel).ok();
        text(f, &BODY, &s, 230, y, Rgb565::CSS_GRAY);
        y += 16;
    }
}
