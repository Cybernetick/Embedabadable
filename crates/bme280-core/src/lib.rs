//! Hardware-independent part of the BME280 driver: decoding the calibration and
//! measurement registers, and the Bosch compensation formulas.
//!
//! Nothing here touches I2C or timers, so it builds for the ESP32 and is unit-tested on
//! the host:
//!
//! ```sh
//! cargo test -p bme280-core --target aarch64-apple-darwin
//! ```
#![cfg_attr(not(test), no_std)]

// `core::fmt` and `std::fmt` are the same module. The host test build goes through `std`
// because `.cargo/config.toml` enables `build-std = ["core"]` for every target, and a
// `core::` path in a std test build would pull in a second `core` (E0152).
#[cfg(not(test))]
use core::fmt::{self, Debug, Formatter};
#[cfg(test)]
use std::fmt::{self, Debug, Formatter};

/// Length of the measurement burst, registers `0xF7..=0xFE`.
pub const MEASUREMENT_LEN: usize = 8;

/// Value a 20-bit channel holds when its measurement was skipped (oversampling = 0).
const SKIPPED_20_BIT: u32 = 0x80000;
/// Value the 16-bit humidity channel holds when its measurement was skipped.
const SKIPPED_16_BIT: u32 = 0x8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TemperatureChannelSkipped,
    PressureChannelSkipped,
    HumidityChannelSkipped,
    /// Pressure compensation would divide by zero. Only happens with bogus calibration
    /// data (e.g. `dig_P1 == 0`), so it usually means the calibration read went wrong.
    DivisionByZero,
}

/// Factory trimming coefficients: the 33 calibration bytes (calib00–25 from `0x88`
/// followed by calib26–32 from `0xE1`), decoded by the getters on each call.
pub struct Calibration {
    raw: [u8; 33]
}

impl Calibration {
    pub fn new(raw: [u8; 33]) -> Self {
        Self { raw }
    }

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

    /// Turns raw ADC values into physical readings, using Bosch's reference formulas:
    /// 32-bit integer for temperature, 64-bit integer for pressure, floating point for
    /// humidity.
    pub fn compensate(&self, raw: &RawMeasurement) -> Result<Readings, Error> {
        let (temperature_centi_c, t_fine) = self.compensate_temperature(raw.temperature);
        let pressure_q24_8 = self.compensate_pressure(raw.pressure, t_fine)?;
        let humidity_percent = self.compensate_humidity(raw.humidity, t_fine);

        Ok(Readings {
            temperature_c: temperature_centi_c as f32 / 100.0,
            pressure_hpa: (pressure_q24_8 as f32 / 256.0) / 100.0,
            humidity_percent,
        })
    }

    /// Returns the temperature in 0.01 °C and `t_fine`, which the pressure and humidity
    /// formulas need as their temperature input.
    fn compensate_temperature(&self, adc_t: u32) -> (i32, i32) {
        let adc_t = adc_t as i32;
        let t1 = self.t1() as i32;
        let t2 = self.t2() as i32;
        let t3 = self.t3() as i32;

        let var1 = (((adc_t >> 3) - (t1 << 1)) * t2) >> 11;
        let var2 = (((((adc_t >> 4) - t1) * ((adc_t >> 4) - t1)) >> 12) * t3) >> 14;
        let t_fine = var1 + var2;
        ((t_fine * 5 + 128) >> 8, t_fine)
    }

    /// Returns the pressure in Pa as unsigned Q24.8 (divide by 256 to get Pa).
    fn compensate_pressure(&self, adc_p: u32, t_fine: i32) -> Result<i64, Error> {
        let mut var1 = t_fine as i64 - 128000;
        let mut var2 = var1 * var1 * self.p6() as i64;
        var2 += (var1 * self.p5() as i64) << 17;
        var2 += (self.p4() as i64) << 35;
        var1 = ((var1 * var1 * self.p3() as i64) >> 8) + ((var1 * self.p2() as i64) << 12);
        var1 = (((1i64 << 47) + var1) * self.p1() as i64) >> 33;
        if var1 == 0 {
            return Err(Error::DivisionByZero);
        }

        let mut p = 1_048_576 - adc_p as i64;
        p = (((p << 31) - var2) * 3125) / var1;
        let var1 = (self.p9() as i64 * (p >> 13) * (p >> 13)) >> 25;
        let var2 = (self.p8() as i64 * p) >> 19;
        Ok(((p + var1 + var2) >> 8) + ((self.p7() as i64) << 4))
    }

    /// Returns the relative humidity in %, clamped to `0.0..=100.0`.
    fn compensate_humidity(&self, adc_h: u32, t_fine: i32) -> f32 {
        let var1 = t_fine as f32 - 76800.0;
        let var2 = self.h4() as f32 * 64.0 + (self.h5() as f32 / 16384.0) * var1;
        let var3 = adc_h as f32 - var2;
        let var4 = self.h2() as f32 / 65536.0;
        let var5 = 1.0 + (self.h3() as f32 / 67_108_864.0) * var1;
        let var6 = 1.0 + (self.h6() as f32 / 67_108_864.0) * var1 * var5;
        let var6 = var3 * var4 * (var5 * var6);

        let humidity = var6 * (1.0 - self.h1() as f32 * var6 / 524_288.0);
        humidity.clamp(0.0, 100.0)
    }
}

impl Debug for Calibration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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

/// Uncompensated ADC values from one measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMeasurement {
    /// 20-bit
    pub pressure: u32,
    /// 20-bit
    pub temperature: u32,
    /// 16-bit
    pub humidity: u32,
}

impl RawMeasurement {
    /// Decodes the burst read from `0xF7..=0xFE` (press_msb … hum_lsb). Fails if any
    /// channel holds its "skipped" marker value.
    pub fn from_bytes(data: &[u8; MEASUREMENT_LEN]) -> Result<Self, Error> {
        let pressure = u32::from_be_bytes([0, data[0], data[1], data[2]]) >> 4;
        let temperature = u32::from_be_bytes([0, data[3], data[4], data[5]]) >> 4;
        let humidity = u32::from_be_bytes([0, 0, data[6], data[7]]);

        if temperature == SKIPPED_20_BIT {
            return Err(Error::TemperatureChannelSkipped);
        }
        if pressure == SKIPPED_20_BIT {
            return Err(Error::PressureChannelSkipped);
        }
        if humidity == SKIPPED_16_BIT {
            return Err(Error::HumidityChannelSkipped);
        }
        Ok(Self { pressure, temperature, humidity })
    }
}

/// Compensated readings in physical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Readings {
    pub temperature_c: f32,
    pub pressure_hpa: f32,
    pub humidity_percent: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calibration coefficients in datasheet order. Test-only: lets tests write values like
    /// `t1: 27504` and encode them into the 33 register bytes `Calibration` stores.
    #[derive(Clone, Copy)]
    struct Coefficients {
        t1: u16, t2: i16, t3: i16,
        p1: u16, p2: i16, p3: i16, p4: i16, p5: i16, p6: i16, p7: i16, p8: i16, p9: i16,
        h1: u8, h2: i16, h3: u8, h4: i16, h5: i16, h6: i8,
    }

    /// T and P coefficients from the worked example in the Bosch BMP280 datasheet (the
    /// BME280 uses the same T/P formulas), plus typical BME280 humidity coefficients.
    const COEFFS: Coefficients = Coefficients {
        t1: 27504, t2: 26435, t3: -1000,
        p1: 36477, p2: -10685, p3: 3024, p4: 2855, p5: 140, p6: -7, p7: 15500, p8: -14600, p9: 6000,
        h1: 75, h2: 362, h3: 0, h4: 313, h5: 50, h6: 30,
    };

    /// Packs coefficients into the register layout: calib00–25 then calib26–32.
    fn encode(c: &Coefficients) -> Calibration {
        let mut raw = [0u8; 33];
        let words: [u16; 12] = [
            c.t1, c.t2 as u16, c.t3 as u16, c.p1, c.p2 as u16, c.p3 as u16,
            c.p4 as u16, c.p5 as u16, c.p6 as u16, c.p7 as u16, c.p8 as u16, c.p9 as u16,
        ];
        for (i, w) in words.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
        }
        raw[25] = c.h1;
        raw[26..28].copy_from_slice(&c.h2.to_le_bytes());
        raw[28] = c.h3;
        raw[29] = (c.h4 >> 4) as u8;
        raw[30] = (c.h4 as u8 & 0x0F) | ((c.h5 as u8 & 0x0F) << 4);
        raw[31] = (c.h5 >> 4) as u8;
        raw[32] = c.h6 as u8;
        Calibration::new(raw)
    }

    #[test]
    fn getters_decode_register_bytes() {
        let cal = encode(&COEFFS);
        let c = COEFFS;
        assert_eq!((cal.t1(), cal.t2(), cal.t3()), (c.t1, c.t2, c.t3));
        assert_eq!(
            [cal.p2(), cal.p3(), cal.p4(), cal.p5(), cal.p6(), cal.p7(), cal.p8(), cal.p9()],
            [c.p2, c.p3, c.p4, c.p5, c.p6, c.p7, c.p8, c.p9]
        );
        assert_eq!(cal.p1(), c.p1);
        assert_eq!((cal.h1(), cal.h2(), cal.h3()), (c.h1, c.h2, c.h3));
        assert_eq!((cal.h4(), cal.h5(), cal.h6()), (c.h4, c.h5, c.h6));
    }

    #[test]
    fn getters_decode_negative_h4_h5_across_shared_nibble() {
        let cal = encode(&Coefficients { h4: -1234, h5: -567, ..COEFFS });
        assert_eq!((cal.h4(), cal.h5()), (-1234, -567));
    }

    #[test]
    fn temperature_matches_datasheet_example() {
        assert_eq!(encode(&COEFFS).compensate_temperature(519888), (2508, 128422));
    }

    #[test]
    fn pressure_matches_reference_implementation() {
        // 25767233 / 256 = 100653.25 Pa; datasheet example gives ≈ 100653 Pa.
        assert_eq!(encode(&COEFFS).compensate_pressure(415148, 128422), Ok(25767233));
    }

    #[test]
    fn humidity_matches_reference_implementation() {
        // Bosch's double-precision formula gives 55.0007 % for these inputs.
        let h = encode(&COEFFS).compensate_humidity(30000, 128422);
        assert!((h - 55.0007).abs() < 0.01, "got {h}");
    }

    #[test]
    fn humidity_is_clamped() {
        let cal = encode(&COEFFS);
        assert_eq!(cal.compensate_humidity(0, 128422), 0.0);
        assert_eq!(cal.compensate_humidity(65535, 128422), 100.0);
    }

    #[test]
    fn compensate_produces_physical_units() {
        let raw = RawMeasurement { pressure: 415148, temperature: 519888, humidity: 30000 };
        let r = encode(&COEFFS).compensate(&raw).unwrap();
        assert!((r.temperature_c - 25.08).abs() < 0.001);
        assert!((r.pressure_hpa - 1006.53).abs() < 0.01);
        assert!((r.humidity_percent - 55.0).abs() < 0.01);
    }

    #[test]
    fn zero_p1_is_division_by_zero() {
        let cal = encode(&Coefficients { p1: 0, ..COEFFS });
        assert_eq!(cal.compensate_pressure(415148, 128422), Err(Error::DivisionByZero));
    }

    #[test]
    fn raw_measurement_decodes_burst() {
        // press 0x6559C, temp 0x7EED0, hum 0x7530, packed as in registers 0xF7..=0xFE.
        let data = [0x65, 0x59, 0xC0, 0x7E, 0xED, 0x00, 0x75, 0x30];
        assert_eq!(
            RawMeasurement::from_bytes(&data),
            Ok(RawMeasurement { pressure: 0x6559C, temperature: 0x7EED0, humidity: 0x7530 })
        );
    }

    #[test]
    fn raw_measurement_detects_skipped_channels() {
        let ok = [0x65, 0x59, 0xC0, 0x7E, 0xED, 0x00, 0x75, 0x30];
        let mut t = ok;
        t[3..6].copy_from_slice(&[0x80, 0x00, 0x00]);
        let mut p = ok;
        p[0..3].copy_from_slice(&[0x80, 0x00, 0x00]);
        let mut h = ok;
        h[6..8].copy_from_slice(&[0x80, 0x00]);
        assert_eq!(RawMeasurement::from_bytes(&t), Err(Error::TemperatureChannelSkipped));
        assert_eq!(RawMeasurement::from_bytes(&p), Err(Error::PressureChannelSkipped));
        assert_eq!(RawMeasurement::from_bytes(&h), Err(Error::HumidityChannelSkipped));
    }
}
