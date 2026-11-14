//! Platform-independent BME280 measurement loop.
//!
//! Embassy tasks can't be generic, so this is a plain `async fn`. Each application wraps it in
//! a small `#[embassy_executor::task]` that picks the concrete bus type.

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;
use log::{info, warn};

use crate::bme280::{Bme280, Readings};

const MEASUREMENT_INTERVAL_MS: u64 = 5000;
const INIT_RETRY_DELAY_MS: u64 = 5000;
/// Consecutive measurement errors tolerated before the sensor is re-initialised.
const MAX_CONSECUTIVE_ERRORS: u32 = 10;

/// Initialises the sensor (retrying until it succeeds), then measures periodically and
/// publishes the readings to `readings`. After too many consecutive errors the driver is
/// dropped and the sensor is initialised again (e.g. after a brown-out or re-plug).
///
/// `make_i2c` is called for every init attempt, because `Bme280::new` takes the device by
/// value and a failed attempt consumes it.
pub async fn run<I, F, M>(mut make_i2c: F, readings: &Signal<M, Readings>)
where
    I: I2c,
    F: FnMut() -> I,
    M: RawMutex,
{
    loop {
        match Bme280::new(make_i2c()).await {
            Ok(mut device) => {
                let mut consecutive_errors: u32 = 0;
                loop {
                    match device.take_force_measurements().await {
                        Ok(reading) => {
                            info!("successfully read data: {:?}", reading);
                            consecutive_errors = 0;
                            readings.signal(reading);
                        }
                        Err(err) => {
                            warn!("failed the readings with error: {:?}", err);
                            consecutive_errors += 1;
                            if consecutive_errors > MAX_CONSECUTIVE_ERRORS {
                                warn!("too many errors, re-initialising the sensor");
                                break;
                            }
                        }
                    }
                    Timer::after_millis(MEASUREMENT_INTERVAL_MS).await;
                }
            }
            Err(err) => {
                warn!("failed to initialize the device with error: {:?}", err);
                Timer::after_millis(INIT_RETRY_DELAY_MS).await;
            }
        }
    }
}
