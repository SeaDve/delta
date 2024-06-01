use std::cell::RefCell;

use anyhow::Result;
use rppal::gpio::{Gpio, Level, OutputPin};

const RED_PIN: u8 = 17;
const GREEN_PIN: u8 = 27;
const BLUE_PIN: u8 = 22;

pub enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug)]
pub struct Led {
    red: RefCell<OutputPin>,
    green: RefCell<OutputPin>,
    blue: RefCell<OutputPin>,
}

impl Drop for Led {
    fn drop(&mut self) {
        self.set_color(None);
    }
}

impl Led {
    pub fn new() -> Result<Self> {
        let gpio = Gpio::new()?;

        Ok(Self {
            red: RefCell::new(gpio.get(RED_PIN)?.into_output()),
            green: RefCell::new(gpio.get(GREEN_PIN)?.into_output()),
            blue: RefCell::new(gpio.get(BLUE_PIN)?.into_output()),
        })
    }

    pub fn set_color(&self, color: Option<Color>) {
        if let Some(color) = color {
            let (r, g, b) = match color {
                Color::Red => (Level::High, Level::Low, Level::Low),
                Color::Green => (Level::Low, Level::High, Level::Low),
                Color::Blue => (Level::Low, Level::Low, Level::High),
            };
            self.red.borrow_mut().write(r);
            self.green.borrow_mut().write(g);
            self.blue.borrow_mut().write(b);
        } else {
            self.red.borrow_mut().set_low();
            self.green.borrow_mut().set_low();
            self.blue.borrow_mut().set_low();
        }
    }
}
