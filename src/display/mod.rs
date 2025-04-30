use crate::slint_backend::EspBackend;
use alloc::boxed::Box;
use alloc::rc::Rc;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::{DrawTarget, OriginDimensions, RgbColor};
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::Peripherals;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::spi::master::{Spi, SpiDmaBus};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Blocking, dma_buffers};
use log::info;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, Display};
use slint::platform::software_renderer::MinimalSoftwareWindow;

// --- Type Alias for the Concrete Display ---
// Use the DMA-enabled SPI bus type.
pub type MyDisplay = Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
    ST7789,
    Output<'static>,
>;

pub fn init_display() -> (Rc<MinimalSoftwareWindow>, MyDisplay) {
    let peripherals = unsafe { Peripherals::steal() };
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
    );
    slint::platform::set_platform(Box::new(EspBackend::new(window.clone())))
        .expect("backend already initialized");
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

    display.clear(Rgb565::BLACK).unwrap();
    // Backlight on GPIO22.
    let mut backlight = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    backlight.set_high();

    info!("Display initialized");

    info!("h:{},w:{}", display.size().height, display.size().width);
    let size = slint::PhysicalSize::new(display.size().width, display.size().height);

    window.set_size(size);

    (window, display)
}
