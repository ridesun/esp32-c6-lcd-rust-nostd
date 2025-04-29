#![no_std]
#![no_main]

extern crate alloc;

#[allow(unused_imports)]
use esp_alloc as _;
#[allow(unused_imports)]
use esp_backtrace as _;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::iter::Cycle;
use core::ops::Range;
use critical_section::Mutex;
use embedded_graphics::prelude::OriginDimensions;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::{DrawTarget, RgbColor};
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::esp_riscv_rt::entry;
use esp_hal::gpio::{Input, InputConfig, Io, Level, Output, OutputConfig, Pull};
use esp_hal::rmt::Rmt;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::spi::master::{Spi, SpiDmaBus};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::{SystemTimer, Unit};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{dma_buffers, handler, Blocking, Config};
use esp_hal_smartled::{smartLedBuffer, SmartLedsAdapter};
use log::info;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, Display};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{brightness, gamma, SmartLedsWrite, RGB8};

// const SSID: &str = env!("SSID");
// const PASSWORD: &str = env!("PASSWORD");
// const STATIC_IP: &str = env!("STATIC_IP");
// const GATEWAY_IP: &str = env!("GATEWAY_IP");
static BUTTON: Mutex<RefCell<Option<MenuHandle>>> = Mutex::new(RefCell::new(None));

// --- Type Alias for the Concrete Display ---
// Use the DMA-enabled SPI bus type.
type MyDisplay = Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
    ST7789,
    Output<'static>,
>;

slint::include_modules!();

#[handler]
fn handler() {
    critical_section::with(|cs| {
        BUTTON.borrow_ref_mut(cs).as_mut().unwrap().handler()
    });
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

    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
    );
    slint::platform::set_platform(Box::new(EspBackend {
        window: window.clone(),
    }))
    .expect("backend already initialized");
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

    let mut rtc = Rtc::new(peripherals.LPWR);
    rtc.rwdt.disable();
    let mut timer_group0 = TimerGroup::new(peripherals.TIMG0);
    timer_group0.wdt.disable();
    let mut timer_group1 = TimerGroup::new(peripherals.TIMG1);
    timer_group1.wdt.disable();

    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(8912);
    let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    // --- Display Setup using BSP values ---
    // SPI: SCK = GPIO7, MOSI = GPIO6, CS = GPIO14.
    let spi = Spi::<Blocking>::new(
        peripherals.SPI2,
        esp_hal::spi::master::Config::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(esp_hal::spi::Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO7)
    .with_mosi(peripherals.GPIO6)
    .with_dma(peripherals.DMA_CH0)
    .with_buffers(dma_rx_buf, dma_tx_buf);

    let cs_output = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    let spi_delay = Delay::new();
    let spi_device = ExclusiveDevice::new(spi, cs_output, spi_delay).unwrap();

    // LCD interface: DC = GPIO15.
    let lcd_dc = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());
    // Leak a Box to obtain a 'static mutable buffer.
    let buffer: &'static mut [u8; 512] = Box::leak(Box::new([0_u8; 512]));
    let di = SpiInterface::new(spi_device, lcd_dc, buffer);

    let mut display_delay = Delay::new();
    display_delay.delay_ns(500_000u32);

    // Reset pin: GPIO21 (active low per BSP).
    let reset = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    // Initialize the display using mipidsi's builder.
    let mut display: MyDisplay = Builder::new(ST7789, di)
        .reset_pin(reset)
        .display_size(206, 320)
        .invert_colors(ColorInversion::Inverted)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .init(&mut display_delay)
        .unwrap();

    display.clear(Rgb565::WHITE).unwrap();
    // Backlight on GPIO22.
    let mut backlight = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    backlight.set_high();

    info!("Display initialized");

    info!("h:{},w:{}", display.size().height, display.size().width);
    let size = slint::PhysicalSize::new(display.size().width, display.size().height);

    window.set_size(size);

    let mut buffer_provider = DrawBuffer {
        display,
        buffer: &mut [slint::platform::software_renderer::Rgb565Pixel::default(); 320],
    };
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
        let menu_index=critical_section::with(|cs| BUTTON.borrow_ref_mut(cs).as_mut().unwrap().menu());
        strong2.global::<Datas>().set_selected_menu_item(menu_index as i32);
        strong2.global::<Datas>().invoke_menu_selected(menu_index as i32);
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut buffer_provider);
        });

        if window.has_active_animations() {
            continue;
        }
    }
}
struct EspBackend {
    window: Rc<MinimalSoftwareWindow>,
}

impl slint::platform::Platform for EspBackend {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        unreachable!()
    }

    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_millis(
            SystemTimer::unit_value(Unit::Unit0) / (SystemTimer::ticks_per_second() / 1000),
        )
    }

    fn debug_log(&self, arguments: core::fmt::Arguments) {
        esp_println::println!("{}", arguments);
    }
}

struct DrawBuffer<'a, MyDisplay> {
    display: MyDisplay,
    buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
}

impl slint::platform::software_renderer::LineBufferProvider for &mut DrawBuffer<'_, MyDisplay> {
    type TargetPixel = slint::platform::software_renderer::Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [slint::platform::software_renderer::Rgb565Pixel]),
    ) {
        let buffer = &mut self.buffer[range.clone()];

        render_fn(buffer);
        // We send empty data just to get the device in the right window
        self.display
            .set_pixels(
                range.start as u16,
                line as _,
                range.end as u16,
                line as u16,
                buffer
                    .iter()
                    .map(|x| embedded_graphics_core::pixelcolor::raw::RawU16::new(x.0).into()),
            )
            .unwrap();
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
    fn menu(&self)->usize{
        self.menu_index
    }
}
