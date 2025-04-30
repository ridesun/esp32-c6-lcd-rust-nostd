use alloc::vec;
use alloc::vec::Vec;
use core::iter::Cycle;
use core::ops::RangeInclusive;
use esp_hal::Blocking;
use esp_hal::gpio::{AnyPin, GpioPin};
use esp_hal::peripherals::RMT;
use esp_hal::rmt::{Channel, Rmt};
use esp_hal::time::Rate;
use esp_hal_smartled::{SmartLedsAdapter, smartLedBuffer};
use smart_leds::hsv::{Hsv, hsv2rgb};
use smart_leds::{SmartLedsWrite, brightness, gamma};

pub struct BoardLed {
    led: SmartLedsAdapter<Channel<Blocking, 0>, 25>,
    color: Hsv,
    hue: Cycle<RangeInclusive<u8>>,
}
impl BoardLed {
    pub fn new(rmt: RMT, led_pin: GpioPin<8>) -> Self {
        let rmt = Rmt::new(rmt, Rate::from_mhz(80)).unwrap();
        let rmt_buffer = smartLedBuffer!(1);
        let led = SmartLedsAdapter::new(rmt.channel0, led_pin, rmt_buffer);
        BoardLed {
            led,
            color: Hsv {
                hue: 0,
                sat: 255,
                val: 255,
            },
            hue: (0..=255).cycle(),
        }
    }
    pub fn blink(&mut self) {
        self.color.hue = self.hue.next().unwrap();
        self.led
            .write(brightness(gamma([hsv2rgb(self.color)].iter().cloned()), 10))
            .unwrap();
    }
}

/// LEDS=amount of led * 24 + 1
pub struct PluginLeds<const LEDS: usize> {
    led: SmartLedsAdapter<Channel<Blocking, 1>, LEDS>,
    color: Vec<Hsv>,
}
impl<const LEDS: usize> PluginLeds<LEDS> {
    pub fn new(rmt: RMT, led_pin: AnyPin) -> Self {
        let rmt = Rmt::new(rmt, Rate::from_mhz(80)).unwrap();
        let rmt_buffer = [0u32; LEDS];
        let led = SmartLedsAdapter::new(rmt.channel1, led_pin, rmt_buffer);
        let color = vec![
            Hsv {
                hue: 0,
                sat: 255,
                val: 255,
            };
            LEDS
        ];
        Self { led, color }
    }
}
