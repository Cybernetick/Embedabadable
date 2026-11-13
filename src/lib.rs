#![no_std]

use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex};
use embassy_sync::signal::Signal;
use crate::bme280::Readings;
use crate::ds1307_clock::DS1307Clock;
static CURRENT_TIME_READINGS: Signal<CriticalSectionRawMutex, DS1307Clock> = Signal::new();

static CURRENT_TEMPERATURE_READINGS: Signal<CriticalSectionRawMutex, Readings> = Signal::new();
pub mod ssd1306_oled;
pub mod ds1307_clock;
pub mod fonts;
pub mod bme280;