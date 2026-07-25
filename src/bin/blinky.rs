#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::gpio::OutputConfig;
use esp_hal::gpio::{Level, Output};
use esp_hal::time::{Duration, Instant};
use esp_println::println;

/*
    populating the application-descriptor
    read the partition table
    conveniently use a partition to read and write flash contents
    I need this because without it the call to espflash flash results in error:
    "ESP-IDF App Descriptor missing in your`esp-hal` application."
*/
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut led_red = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());
    let mut led_blue = Output::new(peripherals.GPIO7, Level::Low, OutputConfig::default());
    println!("Blinky starting...");

    loop {
        led_red.toggle();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
        led_red.toggle();
        led_blue.toggle();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
        led_blue.toggle();
    }
}
