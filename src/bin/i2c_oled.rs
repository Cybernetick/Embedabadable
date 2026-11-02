#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex};
use embassy_sync::mutex::Mutex;
use esp_backtrace as _;
use esp_hal::{Async};
use esp_hal::i2c::master::I2c;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use static_cell::StaticCell;
use Embedabadable::{bme280, ds1307_clock, ssd1306_oled};
use Embedabadable::ds1307_clock::DS1307Clock;

esp_bootloader_esp_idf::esp_app_desc!();

static CELL: StaticCell<Mutex<NoopRawMutex, I2c<Async>>> = StaticCell::new();
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timer = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timer.timer0, sw_interrupt.software_interrupt0);

    let i2c_bus =
        I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
            .expect("Failed to create I2C bus")
            .with_scl(peripherals.GPIO9)
            .with_sda(peripherals.GPIO8);

    let shared = CELL.init(Mutex::new(i2c_bus.into_async()));

    let result = bme280::probe(shared).await;
    match result {
        Ok(_) => {
            println!("bme280 found on probe");
            let soft_reset_result = bme280::soft_reset(shared).await;
            if let Result::Ok(()) = soft_reset_result {
                println!("bme280 soft reset successful");
                let calibration = bme280::read_calibration(shared).await;
                println!("bme280 calibration: {:?}", calibration);
            }
        }
        Err(err) => {
            println!("bme280 not found on probe; {:?}", err);
        }
    }
    ssd1306_oled::run_init_sequence(shared).await;
    
    let is_time_set = ds1307_clock::is_time_set(shared).await;
    if !is_time_set {
        ds1307_clock::set_time(shared, DS1307Clock {
            sec : 30,
            min : 44,
            hour: 18,
            day_of_week: 7,
            day_of_month: 11,
            month: 10,
            year: 2026
        }).await;
    }
    spawner.spawn(ssd1306_oled::draw_clock_time(shared).unwrap());
    spawner.spawn(ds1307_clock::read_time(shared).unwrap())

}
