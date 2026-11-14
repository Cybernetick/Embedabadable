use bme280_core::{Calibration, MEASUREMENT_LEN, RawMeasurement};
pub use bme280_core::Readings;
use embassy_time::Timer;
use embedded_hal_async::i2c::{Error, ErrorKind};

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

pub struct Bme280<I: embedded_hal_async::i2c::I2c> {
    calibration: Calibration,
    i2c_device: I
}

impl <I> Bme280<I> where I: embedded_hal_async::i2c::I2c {
    pub async fn new(mut i2c: I) -> Result<Self, Bme280Error> {
        Self::probe(&mut i2c).await?;
        Self::soft_reset(&mut i2c).await?;
        let calibration_data = Self::read_calibration(&mut i2c).await?;
        Self::configure(&mut i2c).await?;

        Ok(Self {
            i2c_device: i2c,
            calibration: calibration_data
        })
    }

    async fn probe(
        bus: &mut I,
    ) -> Result<(), Bme280Error> {
        let mut answer_buffer: [u8; 1] = [0x00; 1];
        bus.write_read(
            BME280_ADDRESS,
            &[BME280_CHIP_ID_REGISTER],
            &mut answer_buffer,
        ).await.map_err(|err| Bme280Error::I2C(err.kind()))?;

        if answer_buffer[0] == BME280_CHIP_ID {
            Ok(())
        } else {
            Err(Bme280Error::WrongChipId(answer_buffer[0]))
        }
    }

    async fn soft_reset(
        bus: &mut I
    ) -> Result<(), Bme280Error> {
        let payload: [u8; 2] = [BME280_RESET_REGISTER, 0xB6];
        bus.write(BME280_ADDRESS, &payload)
            .await.map_err(|err| Bme280Error::I2C(err.kind()))?;

        Timer::after_millis(5).await;
        let mut is_status_ready = false;
        let mut attempts = 0;
        while !is_status_ready && attempts < 5 {
            attempts += 1;
            let mut status_register_state: [u8; 1] = [0x00; 1];
            let read_result = bus
                .write_read(
                    BME280_ADDRESS,
                    &[BME280_STATUS_REGISTER],
                    &mut status_register_state,
                )
                .await;
            if read_result.is_ok() {
                is_status_ready = status_register_state[0] & 0x01 == 0x00;
            }
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

    async fn read_calibration(bus: &mut I) -> Result<Calibration, Bme280Error> {
        let mut first_calibration_buffer: [u8; 26] = [0x00; 26];
        bus.write_read(
            BME280_ADDRESS,
            &[BME280_CALIBRATION_DATA_REGISTER],
            &mut first_calibration_buffer,
        )
            .await.map_err(|err| Bme280Error::I2C(err.kind()))?;

        let mut second_calibration_buffer: [u8; 7] = [0x00; 7];
        bus.write_read(
            BME280_ADDRESS,
            &[BME280_CALIBRATION_DATA_REGISTER_2],
            &mut second_calibration_buffer,
        )
            .await.map_err(|err| Bme280Error::I2C(err.kind()))?;

        let mut calibration_buffer: [u8; 33] = [0x00; 33];
        calibration_buffer[0..26].copy_from_slice(&first_calibration_buffer);
        calibration_buffer[26..33].copy_from_slice(&second_calibration_buffer);
        Ok(Calibration::new(calibration_buffer))
    }

    async fn configure(bus: &mut I) -> Result<(), Bme280Error> {
        let mut payload: [u8; 2] = [BME280_CONTROL_HUMIDITY_REGISTER, 0x01];
        bus.write(BME280_ADDRESS, &payload).await.map_err(|err| Bme280Error::I2C(err.kind()))?;
        payload[0] = BME280_CONFIG_REGISTER;
        payload[1] = 0x00;
        bus.write(BME280_ADDRESS, &payload).await.map_err(|err| Bme280Error::I2C(err.kind()))?;
        payload[0] = BME280_CONTROL_TEMP_PRESSURE_REGISTER;
        /*
            7:5 temperature oversampling, bits 4:2 pressure oversampling, bits 1:0 mode (00 sleep, 01 forced, 11 normal).
        */
        payload[1] = 0b001_001_00;
        bus.write(BME280_ADDRESS, &payload).await.map_err(|err| Bme280Error::I2C(err.kind()))?;
        Ok(())
    }

    pub async fn take_force_measurements(&mut self) -> Result<Readings, Bme280Error> {
        let payload: [u8; 2] = [BME280_CONTROL_TEMP_PRESSURE_REGISTER, 0b001_001_01];
        self.i2c_device.write(BME280_ADDRESS, &payload).await.map_err(|err| Bme280Error::I2C(err.kind()))?;
        Timer::after_millis(10).await;
        let mut data = [0x00; MEASUREMENT_LEN];
        self.i2c_device.write_read(BME280_ADDRESS, &[BME280_PRESSURE_MSB_REGISTER], &mut data)
            .await
            .map_err(|err| Bme280Error::I2C(err.kind()))?;

        let raw = RawMeasurement::from_bytes(&data)?;
        Ok(self.calibration.compensate(&raw)?)
    }
}

#[derive(Debug)]
pub enum Bme280Error {
    I2C(ErrorKind),
    WrongChipId(u8),
    Timeout,
    Compensation(bme280_core::Error)
}

impl From<bme280_core::Error> for Bme280Error {
    fn from(value: bme280_core::Error) -> Self {
        Bme280Error::Compensation(value)
    }
}
