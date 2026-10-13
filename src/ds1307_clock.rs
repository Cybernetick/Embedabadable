use crate::CURRENT_TIME_READINGS;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::i2c::master::{I2c, I2cAddress};
use esp_hal::Async;
use esp_println::println;
use rtc_core::bcd::{bcd2dec, dec2bcd};

const DS1307_FLAG_ADDRESS: u8 = 0x0A;
const DS1307_I2C_ADDRESS: u8 = 0x68;
const DS1307_FLAG_TIME_SET_VALUE: u8 = 0xAA;
pub async fn is_time_set(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> bool {
    let mut bus_ = bus.lock().await;
    let mut buffer:[u8; 1] = [0x00; 1];
    let result = bus_.write_read_async(I2cAddress::from(DS1307_I2C_ADDRESS), &[DS1307_FLAG_ADDRESS], &mut buffer).await;
    match result {
        Ok(_) => {
            if buffer[0] == DS1307_FLAG_TIME_SET_VALUE {
                true
            } else {
                false
            }
        }
        Err(err) => {
            println!("DS1307: failed to read flag register: {:?}", err);
            false
        }
    }
}

pub async fn set_time(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>, time: DS1307Clock) {
    let mut buffer = [0x00; 8];
    buffer[0] = 0x00;
    buffer[1] = dec2bcd(time.sec);
    buffer[2] = dec2bcd(time.min);
    buffer[3] = dec2bcd(time.hour);
    buffer[4] = dec2bcd(time.day_of_week);
    buffer[5] = dec2bcd(time.day_of_month);
    buffer[6] = dec2bcd(time.month);
    buffer[7] = dec2bcd( (time.year % 100) as u8);

    let mut bus_ = bus.lock().await;
    let result = bus_.write_async(I2cAddress::from(DS1307_I2C_ADDRESS), &buffer).await;
    match result {
        Ok(_) => {
            println!("DS1307: time set");
            let mut flag_set_buffer = [0x00; 2];
            flag_set_buffer[0] = DS1307_FLAG_ADDRESS;
            flag_set_buffer[1] = DS1307_FLAG_TIME_SET_VALUE;
            bus_.write_async(I2cAddress::from(DS1307_I2C_ADDRESS), &flag_set_buffer).await.unwrap();
        }
        Err(err) => {
            println!("DS1307: failed to set time: {:?}", err);
        }
    }
}

#[embassy_executor::task]
pub async fn read_time(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) {
    let mut last_sent_value = Option::<DS1307Clock>::None;
    loop {
        let mut buffer:[u8; 7] = [0x00; 7];
        let mut bus_ = bus.lock().await;
        let result = bus_.write_read_async(I2cAddress::from(DS1307_I2C_ADDRESS), &[0x00], &mut buffer).await;

        match result {

            Ok(_) => {
                let update = DS1307Clock {
                    sec: bcd2dec(buffer[0] & 0x7F),
                    min: bcd2dec(buffer[1]),
                    hour: bcd2dec(buffer[2] & 0x3F),
                    day_of_week: bcd2dec(buffer[3]),
                    day_of_month: bcd2dec(buffer[4]),
                    month: bcd2dec(buffer[5]),
                    year: bcd2dec(buffer[6]) as u16 + 2000,
                };
                let value_updated = last_sent_value.map_or(true, |last_value| last_value.sec != update.sec);
                if value_updated {
                    last_sent_value = Some(update);
                    CURRENT_TIME_READINGS.signal(update);
                }
            }
            Err(err) => {
                println!("DS1307: failed to read time: {:?}", err);
            },
        };
        drop(bus_);
        Timer::after_millis(250).await;
    }
}


#[derive(Copy, Clone)]
pub struct DS1307Clock {
    pub sec: u8,
    pub min: u8,
    pub hour: u8,
    pub day_of_week: u8,
    pub day_of_month: u8,
    pub month: u8,
    pub year: u16,
}