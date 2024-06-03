use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::Result;
use gtk::glib::{self, clone};
use rppal::gpio::{Gpio, Level, OutputPin};

#[derive(Debug, Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
}

#[derive(Debug)]
pub struct Led(Rc<RefCell<Inner>>);

#[derive(Debug)]
struct Inner {
    red: OutputPin,
    green: OutputPin,
    blue: OutputPin,
    blink_source_id: Option<glib::SourceId>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.set_color(None);
    }
}

impl Inner {
    fn set_color(&mut self, color: Option<Color>) {
        if let Some(color) = color {
            let (r, g, b) = match color {
                Color::Red => (Level::High, Level::Low, Level::Low),
                Color::Green => (Level::Low, Level::High, Level::Low),
                Color::Blue => (Level::Low, Level::Low, Level::High),
                Color::Yellow => (Level::High, Level::High, Level::Low),
            };
            self.red.write(r);
            self.green.write(g);
            self.blue.write(b);
        } else {
            self.red.set_low();
            self.green.set_low();
            self.blue.set_low();
        }
    }
}

impl Led {
    pub fn new(red_pin: u8, green_pin: u8, blue_pin: u8) -> Result<Self> {
        let gpio = Gpio::new()?;

        Ok(Self(Rc::new(RefCell::new(Inner {
            red: gpio.get(red_pin)?.into_output(),
            green: gpio.get(green_pin)?.into_output(),
            blue: gpio.get(blue_pin)?.into_output(),
            blink_source_id: None,
        }))))
    }

    pub fn set_color(&self, color: Option<Color>) {
        self.0.borrow_mut().set_color(color);
    }

    pub fn blink(&self, color: Color, repeat_count: u32, interval: Duration) {
        let mut inner_mut = self.0.borrow_mut();

        if let Some(source_id) = inner_mut.blink_source_id.take() {
            inner_mut.set_color(None);
            source_id.remove();
        }

        let mut repeat_count = repeat_count * 2;
        let inner = &self.0;

        let source_id = glib::timeout_add_local_full(
            interval,
            glib::Priority::DEFAULT_IDLE,
            clone!(@weak inner => @default-panic, move || {
                let mut inner_mut = inner.borrow_mut();

                if repeat_count % 2 == 0 {
                    inner_mut.set_color(Some(color));
                } else {
                    inner_mut.set_color(None);
                }

                repeat_count -= 1;

                if repeat_count == 0 {
                    inner_mut.set_color(None);
                    inner_mut.blink_source_id = None;
                    return glib::ControlFlow::Break;
                }

                glib::ControlFlow::Continue
            }),
        );
        inner_mut.blink_source_id = Some(source_id);
    }
}
