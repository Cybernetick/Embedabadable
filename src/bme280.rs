use core::fmt::{Debug, Formatter};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::Async;
use esp_hal::i2c::master::{Error, I2c, I2cAddress};
use esp_println::println;

const BME280_ADDRESS: u8 = 0x76;
const BME280_CHIP_ID: u8 = 0x60;
const BME280_RESET_REGISTER: u8 = 0xE0;
const BME280_CHIP_ID_REGISTER: u8 = 0xD0;
const BME280_STATUS_REGISTER: u8 = 0xF3;
const BME280_CALIBRATION_DATA_REGISTER: u8 = 0x88;

const BME280_CALIBRATION_DATA_REGISTER_2: u8 = 0xE1;

pub async fn probe(
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

pub async fn soft_reset(
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

pub async fn read_calibration(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> Result<Calibration, Bme280Error> {

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
    Ok(Calibration { raw: calibration_buffer })
}

pub struct Calibration {
    raw: [u8; 33]
}

impl Calibration {
    pub fn t1(&self) -> u16 {
        u16::from_le_bytes([self.raw[0], self.raw[1]])
    }

    pub fn t2(&self) -> i16 {
        i16::from_le_bytes([self.raw[2], self.raw[3]])
    }

    pub fn t3(&self) -> i16 {
        i16::from_le_bytes([self.raw[4], self.raw[5]])
    }

    pub fn p1(&self) -> u16 {
        u16::from_le_bytes([self.raw[6], self.raw[7]])
    }

    pub fn p2(&self) -> i16 {
        i16::from_le_bytes([self.raw[8], self.raw[9]])
    }

    pub fn p3(&self) -> i16 {
        i16::from_le_bytes([self.raw[10], self.raw[11]])
    }

    pub fn p4(&self) -> i16 {
        i16::from_le_bytes([self.raw[12], self.raw[13]])
    }

    pub fn p5(&self) -> i16 {
        i16::from_le_bytes([self.raw[14], self.raw[15]])
    }

    pub fn p6(&self) -> i16 {
        i16::from_le_bytes([self.raw[16], self.raw[17]])
    }

    pub fn p7(&self) -> i16 {
        i16::from_le_bytes([self.raw[18], self.raw[19]])
    }

    pub fn p8(&self) -> i16 {
        i16::from_le_bytes([self.raw[20], self.raw[21]])
    }

    pub fn p9(&self) -> i16 {
        i16::from_le_bytes([self.raw[22], self.raw[23]])
    }

    pub fn h1(&self) -> u8 {
        self.raw[25]
    }

    pub fn h2(&self) -> i16 {
        i16::from_le_bytes([self.raw[26], self.raw[27]])
    }

    pub fn h3(&self) -> u8 {
        self.raw[28]
    }

    pub fn h4(&self) -> i16 {
        ((self.raw[29] as i8 as i16) << 4) | (self.raw[30] & 0x0F) as i16
    }

    pub fn h5(&self) -> i16 {
        // i16::from_le_bytes([self.raw[31] << 4, self.raw[30] >> 4])
        ((self.raw[31] as i8 as i16) << 4) | (self.raw[30] >> 4) as i16
    }

    pub fn h6(&self) -> i8 {
        self.raw[32] as i8
    }
}

impl Debug for Calibration {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Calibration")
            .field("t1", &self.t1())
            .field("t2", &self.t2())
            .field("t3", &self.t3())
            .field("p1", &self.p1())
            .field("p2", &self.p2())
            .field("p3", &self.p3())
            .field("p4", &self.p4())
            .field("p5", &self.p5())
            .field("p6", &self.p6())
            .field("p7", &self.p7())
            .field("p8", &self.p8())
            .field("p9", &self.p9())
            .field("h1", &self.h1())
            .field("h2", &self.h2())
            .field("h3", &self.h3())
            .field("h4", &self.h4())
            .field("h5", &self.h5())
            .field("h6", &self.h6())
            .finish()
    }
}

#[derive(Debug)]
pub enum Bme280Error {
    I2CError(Error),
    WrongChipId(u8),
    Timeout,
}

impl From<esp_hal::i2c::master::Error> for Bme280Error {
    fn from(value: Error) -> Self {
        Bme280Error::I2CError(value)
    }
}
