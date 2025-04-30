#![no_std]
#![no_main]

mod display;
mod led;
mod slint_backend;

extern crate alloc;

#[allow(unused_imports)]
use esp_alloc as _;
#[allow(unused_imports)]
use esp_backtrace as _;

use crate::led::BoardLed;
use crate::slint_backend::DrawBuffer;
use core::cell::RefCell;
use core::iter::Cycle;
use core::ops::Range;
use critical_section::Mutex;
use esp_hal::esp_riscv_rt::entry;
use esp_hal::gpio::{Input, InputConfig, Io, Pull};
use esp_hal::{Config, handler};
use log::info;

// const SSID: &str = env!("SSID");
// const PASSWORD: &str = env!("PASSWORD");
// const STATIC_IP: &str = env!("STATIC_IP");
// const GATEWAY_IP: &str = env!("GATEWAY_IP");
static MENU_BUTTON: Mutex<RefCell<Option<MenuHandle>>> = Mutex::new(RefCell::new(None));

slint::include_modules!();

#[handler]
fn menu_button_handler() {
    critical_section::with(|cs| MENU_BUTTON.borrow_ref_mut(cs).as_mut().unwrap().handler());
}

#[entry]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size:150 * 1024);

    let peripherals = esp_hal::init(Config::default());

    let led_pin = peripherals.GPIO8;
    let rmt = peripherals.RMT;
    let mut board_led = BoardLed::new(rmt, led_pin);

    let (window, display) = display::init_display();
    let mut buffer_provider = DrawBuffer {
        display,
        buffer: &mut [slint::platform::software_renderer::Rgb565Pixel::default(); 320],
    };

    let ui = HelloWorld::new().unwrap();
    let ui_strong = ui.clone_strong();
    let datas = ui_strong.global::<Datas>();

    let timer_led = slint::Timer::default();
    timer_led.start(
        slint::TimerMode::Repeated,
        core::time::Duration::from_millis(50),
        move || {
            board_led.blink();
        },
    );

    info!("start ui");

    ui.show().unwrap();

    let input_config = InputConfig::default().with_pull(Pull::Up);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(menu_button_handler);
    let mut gpio9 = Input::new(peripherals.GPIO9, input_config);
    gpio9.listen(esp_hal::gpio::Event::FallingEdge);
    let menu_handle = MenuHandle {
        input: gpio9,
        menu_index: 0,
        menu_range: (0..4).cycle(),
    };
    critical_section::with(|cs| MENU_BUTTON.borrow_ref_mut(cs).replace(menu_handle));

    loop {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut buffer_provider);
        });

        if window.has_active_animations() {
            continue;
        }

        let menu_index =
            critical_section::with(|cs| MENU_BUTTON.borrow_ref_mut(cs).as_mut().unwrap().menu());

        datas.set_selected_menu_item(menu_index as i32);
        datas.invoke_menu_selected(menu_index as i32);
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
