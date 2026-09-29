//! Four pages cycled with the BOOT button; KEY cycles backlight brightness.
//!
//! 1. Greeting with falling confetti
//! 1. Plasma animation with FPS counter
//! 1. System info (chip, memory, battery, uptime)
//! 1. WiFi scan

#![no_std]
#![no_main]

mod pages;

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use esp_alloc::MemoryCapability;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Input;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, WifiController};
use heapless::Vec;
use lilygo_t_display_s3::{Board, FrameBuffer, resources};
use log::{info, warn};
use pages::{AccessPoint, App, Page, SystemInfo, WifiView};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

const INTERNAL_HEAP: usize = 73744;
const BRIGHTNESS_PCT: [u8; 4] = [100, 63, 31, 9];
const MAX_NETWORKS: usize = 12;

type Networks = Vec<AccessPoint, MAX_NETWORKS>;

/// Latest scan results; `None` until the first scan completes.
static NETWORKS: Mutex<CriticalSectionRawMutex, RefCell<Option<Networks>>> =
    Mutex::new(RefCell::new(None));
/// Only scan while someone is looking at the WiFi page.
static WIFI_PAGE_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Press detection for an active-low button, with a 30 ms debounce.
struct Debounce {
    last_high: bool,
    changed_at: Instant,
}

impl Debounce {
    fn new() -> Self {
        Self {
            last_high: true,
            changed_at: Instant::now(),
        }
    }

    /// True once per press.
    fn pressed(&mut self, pin: &Input) -> bool {
        let high = pin.is_high();
        if high != self.last_high && self.changed_at.elapsed() > Duration::from_millis(30) {
            self.changed_at = Instant::now();
            self.last_high = high;
            return !high;
        }
        false
    }
}

#[embassy_executor::task]
async fn wifi_scan(mut controller: WifiController<'static>) {
    if let Err(err) = controller.set_config(&WifiConfig::Station(StationConfig::default())) {
        warn!("WiFi station mode failed: {err:?}");
        return;
    }

    loop {
        if !WIFI_PAGE_VISIBLE.load(Ordering::Relaxed) {
            Timer::after(Duration::from_millis(200)).await;
            continue;
        }

        match controller
            .scan_async(&ScanConfig::default().with_max(MAX_NETWORKS))
            .await
        {
            Ok(found) => NETWORKS.lock(|n| {
                // Fill the shared list in place rather than building a copy on the stack.
                let mut n = n.borrow_mut();
                let networks = n.insert(Networks::new());
                for ap in found.iter().take(MAX_NETWORKS) {
                    networks
                        .push(AccessPoint {
                            ssid: heapless::String::try_from(ap.ssid.as_str()).unwrap_or_default(),
                            rssi: ap.signal_strength,
                            channel: ap.channel,
                        })
                        .ok();
                }
                networks.sort_unstable_by_key(|ap| core::cmp::Reverse(ap.rssi));
                info!("WiFi scan found {} networks", networks.len());
            }),
            Err(err) => warn!("WiFi scan failed: {err:?}"),
        }

        Timer::after(Duration::from_secs(10)).await;
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: INTERNAL_HEAP);
    let psram = esp_hal::psram::Psram::new(peripherals.PSRAM, Default::default());
    let (_, psram_size) = psram.raw_parts();
    esp_alloc::psram_allocator!(&psram);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let mut board = Board::new(resources!(peripherals)).expect("board init");
    info!("lilygo-t-display-s3 demo {}", env!("CARGO_PKG_VERSION"));

    // Too big for internal RAM next to WiFi, so this lands in PSRAM.
    let pixels = alloc::vec![Rgb565::BLACK; FrameBuffer::LEN].leak();
    let mut frame = FrameBuffer::new(pixels);

    let controller = WifiController::new(peripherals.WIFI, Default::default())
        .expect("Failed to initialize Wi-Fi controller");
    spawner.spawn(wifi_scan(controller).expect("spawn wifi task"));

    let mac: [u8; 6] = esp_hal::efuse::base_mac_address()
        .as_bytes()
        .try_into()
        .unwrap_or([0; 6]);
    let chip_revision = (
        esp_hal::efuse::major_chip_version(),
        esp_hal::efuse::minor_chip_version(),
    );

    let mut app = App::new(Rng::new().random());
    let mut boot = Debounce::new();
    let mut key = Debounce::new();
    let mut brightness = 0;
    let mut battery_mv = 0;
    let mut last_battery_read = Instant::from_ticks(0);
    let mut fps = 0.0;
    let mut fps_window_start = Instant::now();
    let mut frames_in_window = 0u32;
    let mut last_fps_log = Instant::now();

    loop {
        if boot.pressed(&board.boot_button) {
            app.page = app.page.next();
            WIFI_PAGE_VISIBLE.store(app.page == Page::Wifi, Ordering::Relaxed);
            info!("BOOT pressed, page {:?}", app.page);
        }
        if key.pressed(&board.key_button) {
            brightness = (brightness + 1) % BRIGHTNESS_PCT.len();
            board.backlight.set_percent(BRIGHTNESS_PCT[brightness]);
            info!("KEY pressed, brightness {}%", BRIGHTNESS_PCT[brightness]);
        }

        if last_battery_read.elapsed() > Duration::from_millis(500) {
            battery_mv = board.battery.millivolts();
            last_battery_read = Instant::now();
        }

        frames_in_window += 1;
        let window = fps_window_start.elapsed();
        if window >= Duration::from_secs(1) {
            fps = frames_in_window as f32 * 1000.0 / window.as_millis() as f32;
            if last_fps_log.elapsed() >= Duration::from_secs(10) {
                info!("{:?} page: {fps:.1} fps", app.page);
                last_fps_log = Instant::now();
            }
            frames_in_window = 0;
            fps_window_start = Instant::now();
        }

        let info = SystemInfo {
            chip_revision,
            cpu_mhz: 240,
            heap_free: esp_alloc::HEAP.free_caps(MemoryCapability::Internal.into()),
            heap_size: INTERNAL_HEAP,
            psram_free: esp_alloc::HEAP.free_caps(MemoryCapability::External.into()),
            psram_size,
            mac,
            uptime_secs: Instant::now().as_secs(),
            battery_mv,
            build: concat!("lilygo-t-display-s3 ", env!("CARGO_PKG_VERSION")),
            fps,
        };
        let networks = if app.page == Page::Wifi {
            NETWORKS.lock(|n| n.borrow().clone())
        } else {
            None
        };
        let wifi = WifiView {
            scanned: networks.is_some(),
            networks: networks.as_deref().unwrap_or(&[]),
        };

        app.render(&mut frame, &info, &wifi);
        frame.flush(&mut board.display).expect("display flush");

        // Plasma runs flat out; the other pages only need to look smooth.
        let pause = if app.page == Page::Plasma { 1 } else { 12 };
        Timer::after(Duration::from_millis(pause)).await;
    }
}
