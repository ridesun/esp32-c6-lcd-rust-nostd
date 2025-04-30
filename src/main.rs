#![no_std]
#![no_main]

mod display;
mod slint_backend;

extern crate alloc;

#[allow(unused_imports)]
use esp_alloc as _;
#[allow(unused_imports)]
use esp_backtrace as _;

use crate::slint_backend::DrawBuffer;
use core::cell::RefCell;
use core::iter::Cycle;
use core::ops::Range;
use critical_section::Mutex;
use esp_hal::esp_riscv_rt::entry;
use esp_hal::gpio::{Input, InputConfig, Io, Pull};
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::{handler, Config};
use esp_hal_smartled::{smartLedBuffer, SmartLedsAdapter};
use log::info;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{brightness, gamma, SmartLedsWrite};

// const SSID: &str = env!("SSID");
// const PASSWORD: &str = env!("PASSWORD");
// const STATIC_IP: &str = env!("STATIC_IP");
// const GATEWAY_IP: &str = env!("GATEWAY_IP");
static BUTTON: Mutex<RefCell<Option<MenuHandle>>> = Mutex::new(RefCell::new(None));

slint::include_modules!();

#[handler]
fn handler() {
    critical_section::with(|cs| BUTTON.borrow_ref_mut(cs).as_mut().unwrap().handler());
}

#[entry]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size:150 * 1024);

    let peripherals = esp_hal::init(Config::default());

    let led_pin = peripherals.GPIO8;
    let rmt = peripherals.RMT;
    let rmt = Rmt::new(rmt, Rate::from_mhz(80)).unwrap();
    let rmt_buffer = smartLedBuffer!(1);
    let mut led = SmartLedsAdapter::new(rmt.channel0, led_pin, rmt_buffer);

    let mut color = Hsv {
        hue: 0,
        sat: 255,
        val: 255,
    };
    let mut hue = (0..=255).cycle();

    let (window, display) = display::init_display();
    let mut buffer_provider = DrawBuffer {
        display,
        buffer: &mut [slint::platform::software_renderer::Rgb565Pixel::default(); 320],
    };

    let ui = HelloWorld::new().unwrap();
    let strong = ui.clone_strong();
    let strong2 = ui.clone_strong();
    let timer1 = slint::Timer::default();
    timer1.start(
        slint::TimerMode::SingleShot,
        core::time::Duration::from_millis(1000),
        move || {
            strong.global::<Datas>().set_text("Init Success".into());
        },
    );

    let timer2 = slint::Timer::default();
    timer2.start(
        slint::TimerMode::Repeated,
        core::time::Duration::from_millis(50),
        move || {
            color.hue = hue.next().unwrap();

            led.write(brightness(gamma([hsv2rgb(color)].iter().cloned()), 10))
                .unwrap();
        },
    );

    info!("start ui");

    ui.show().unwrap();

    let input_config = InputConfig::default().with_pull(Pull::Up);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(handler);
    let mut gpio9 = Input::new(peripherals.GPIO9, input_config);
    gpio9.listen(esp_hal::gpio::Event::FallingEdge);
    let menu_handle = MenuHandle {
        input: gpio9,
        menu_index: 0,
        menu_range: (0..4).cycle(),
    };
    critical_section::with(|cs| BUTTON.borrow_ref_mut(cs).replace(menu_handle));

    loop {
        slint::platform::update_timers_and_animations();
        let menu_index =
            critical_section::with(|cs| BUTTON.borrow_ref_mut(cs).as_mut().unwrap().menu());
        strong2
            .global::<Datas>()
            .set_selected_menu_item(menu_index as i32);
        strong2
            .global::<Datas>()
            .invoke_menu_selected(menu_index as i32);
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut buffer_provider);
        });

        if window.has_active_animations() {
            continue;
        }
    }
}

struct MenuHandle<'a> {
    input: Input<'a>,
    menu_index: usize,
    menu_range: Cycle<Range<usize>>,
}
impl<'a> MenuHandle<'a> {
    fn handler(&mut self) {
        self.input.clear_interrupt();
        self.menu_index = self.menu_range.next().unwrap();
    }
    fn menu(&self) -> usize {
        self.menu_index
    }
}
