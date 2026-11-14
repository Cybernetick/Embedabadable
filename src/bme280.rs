use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use bme280_core::{Calibration, MEASUREMENT_LEN, RawMeasurement};
pub use bme280_core::Readings;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::Async;
use esp_hal::i2c::master::{Error, I2c, I2cAddress};
use esp_println::println;
use crate::CURRENT_TEMPERATURE_READINGS;

const BME280_ADDRESS: u8 = 0x76;
const BME280_CHIP_ID: u8 = 0x60;
const BME280_RESET_REGISTER: u8 = 0xE0;
const BME280_CHIP_ID_REGISTER: u8 = 0xD0;
const BME280_STATUS_REGISTER: u8 = 0xF3;
const BME280_CALIBRATION_DATA_REGISTER: u8 = 0x88;

const BME280_CALIBRATION_DATA_REGISTER_2: u8 = 0xE1;
const BME280_CONTROL_HUMIDITY_REGISTER: u8 = 0xF2;
const BME280_CONFIG_REGISTER: u8 = 0xF5;
const BME280_CONTROL_TEMP_PRESSURE_REGISTER: u8 = 0xF4;
const BME280_PRESSURE_MSB_REGISTER: u8 = 0xF7;

pub struct Bme280 {
    i2c_bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>,
    calibration: Calibration,
}

impl Bme280 {
    pub async fn new(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> Result<Self, Bme280Error> {
        Self::probe(bus).await?;
        Self::soft_reset(bus).await?;
        let calibration_data = Self::read_calibration(bus).await?;
        Self::configure(bus).await?;

        Ok(Self {
            i2c_bus: bus,
            calibration: calibration_data
        })
    }

    async fn probe(
        bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>,
    ) -> Result<(), Bme280Error> {
        let mut lock = bus.lock().await;
        let mut answer_buffer: [u8; 1] = [0x00; 1];
        lock.write_read_async(
            I2cAddress::from(BME280_ADDRESS),
            &[BME280_CHIP_ID_REGISTER],
            &mut answer_buffer,
        )
            .await?;

        if answer_buffer[0] == BME280_CHIP_ID {
            Ok(())
        } else {
            Err(Bme280Error::WrongChipId(answer_buffer[0]))
        }
    }

    async fn soft_reset(
        bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>,
    ) -> Result<(), Bme280Error> {
        let mut lock = bus.lock().await;
        let payload: [u8; 2] = [BME280_RESET_REGISTER, 0xB6];
        lock.write_async(I2cAddress::from(BME280_ADDRESS), &payload)
            .await?;

        drop(lock);

        Timer::after_millis(5).await;
        let mut is_status_ready = false;
        let mut attempts = 0;
        while !is_status_ready && attempts < 5 {
            let mut lock = bus.lock().await;
            attempts += 1;
            let mut status_register_state: [u8; 1] = [0x00; 1];
            let read_result = lock
                .write_read_async(
                    I2cAddress::from(BME280_ADDRESS),
                    &[BME280_STATUS_REGISTER],
                    &mut status_register_state,
                )
                .await;
            if read_result.is_ok() {
                is_status_ready = status_register_state[0] & 0x01 == 0x00;
            }
            drop(lock);
            if !is_status_ready && attempts < 5 {
                Timer::after_millis(5).await;
            }
        }

        if is_status_ready {
            Ok(())
        } else {
            Err(Bme280Error::Timeout)
        }
    }

    async fn read_calibration(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> Result<Calibration, Bme280Error> {
        let mut lock = bus.lock().await;
        let mut first_calibration_buffer: [u8; 26] = [0x00; 26];
        lock.write_read_async(
            I2cAddress::from(BME280_ADDRESS),
            &[BME280_CALIBRATION_DATA_REGISTER],
            &mut first_calibration_buffer,
        )
            .await?;

        let mut second_calibration_buffer: [u8; 7] = [0x00; 7];
        lock.write_read_async(
            I2cAddress::from(BME280_ADDRESS),
            &[BME280_CALIBRATION_DATA_REGISTER_2],
            &mut second_calibration_buffer,
        )
            .await?;

        let mut calibration_buffer: [u8; 33] = [0x00; 33];
        calibration_buffer[0..26].copy_from_slice(&first_calibration_buffer);
        calibration_buffer[26..33].copy_from_slice(&second_calibration_buffer);
        Ok(Calibration::new(calibration_buffer))
    }

    async fn configure(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> Result<(), Bme280Error> {
        let mut lock = bus.lock().await;
        let mut payload: [u8; 2] = [BME280_CONTROL_HUMIDITY_REGISTER, 0x01];
        lock.write_async(I2cAddress::from(BME280_ADDRESS), &payload).await?;
        payload[0] = BME280_CONFIG_REGISTER;
        payload[1] = 0x00;
        lock.write_async(I2cAddress::from(BME280_ADDRESS), &payload).await?;
        payload[0] = BME280_CONTROL_TEMP_PRESSURE_REGISTER;
        /*
            7:5 temperature oversampling, bits 4:2 pressure oversampling, bits 1:0 mode (00 sleep, 01 forced, 11 normal).
        */
        payload[1] = 0b001_001_00;
        lock.write_async(I2cAddress::from(BME280_ADDRESS), &payload).await?;
        Ok(())
    }

    pub async fn take_force_measurements(&self) -> Result<Readings, Bme280Error> {
        let mut lock = self.i2c_bus.lock().await;
        let payload: [u8; 2] = [BME280_CONTROL_TEMP_PRESSURE_REGISTER, 0b001_001_01];
        lock.write_async(I2cAddress::from(BME280_ADDRESS), &payload).await?;
        drop(lock);
        Timer::after_millis(10).await;
        lock = self.i2c_bus.lock().await;
        let mut data = [0x00; MEASUREMENT_LEN];
        lock.write_read_async(I2cAddress::from(BME280_ADDRESS), &[BME280_PRESSURE_MSB_REGISTER], &mut data).await?;
        drop(lock);

        let raw = RawMeasurement::from_bytes(&data)?;
        Ok(self.calibration.compensate(&raw)?)
    }
}

#[embassy_executor::task]
pub async fn measurements_task(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) {
    let mut measurements_error_count = 0;
    loop {
        let device = Bme280::new(bus).await;
        match device {
            Ok(device) => {
                measurements_error_count = 0;
                loop {
                    let measurements = device.take_force_measurements().await;
                    match measurements {
                        Ok(readings) => {
                            println!("successfully read data: {:?}", readings);
                            measurements_error_count = 0;
                            CURRENT_TEMPERATURE_READINGS.signal(readings);
                        }
                        Err(err) => {
                            println!("failed the readings with error: {:?}", err);
                            measurements_error_count += 1;
                            if measurements_error_count > 10 {
                                println!("too many errors, re-initialising the sensor");
                                break;
                            }
                        }
                    }
                    Timer::after_millis(5000).await;
                }
            }
            Err(error) => {
                println!("failed to initialize the device with error: {:?}", error);
                Timer::after_millis(5000).await;
            }
        }
    }

}

#[derive(Debug)]
pub enum Bme280Error {
    I2CError(Error),
    WrongChipId(u8),
    Timeout,
    Compensation(bme280_core::Error)
}

impl From<Error> for Bme280Error {
    fn from(value: Error) -> Self {
        Bme280Error::I2CError(value)
    }
}

impl From<bme280_core::Error> for Bme280Error {
    fn from(value: bme280_core::Error) -> Self {
        Bme280Error::Compensation(value)
    }
}
