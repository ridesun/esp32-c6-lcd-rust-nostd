use crate::display::MyDisplay;
use alloc::rc::Rc;
use core::ops::Range;
use esp_hal::timer::systimer::{SystemTimer, Unit};
use slint::platform::software_renderer::MinimalSoftwareWindow;

pub struct EspBackend {
    window: Rc<MinimalSoftwareWindow>,
}
impl EspBackend {
    pub fn new(window: Rc<MinimalSoftwareWindow>) -> Self {
        Self { window }
    }
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

pub struct DrawBuffer<'a, MyDisplay> {
    pub display: MyDisplay,
    pub buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
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
