#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::gpio::DriveMode;
use esp_hal::ledc::{channel, timer, LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::ledc::channel::{Channel, ChannelIFace};
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::time::Rate;

esp_bootloader_esp_idf::esp_app_desc!();
#[esp_hal::main]
fn main() ->! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let led = peripherals.GPIO6;
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut ls_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    ls_timer.configure(timer::config::Config {
        duty: timer::config::Duty::Duty5Bit,
        clock_source: timer::LSClockSource::APBClk,
        frequency: Rate::from_khz(24),
    }).unwrap();

    let mut channel = ledc.channel::<LowSpeed>(channel::Number::Channel0, led);
    channel.configure(channel::config::Config {
        timer: &ls_timer,
        duty_pct: 10,
        drive_mode: DriveMode::PushPull,
    }).unwrap();

    loop {
        channel.start_duty_fade(0, 100, 1000).unwrap();
        while channel.is_duty_fade_running() {}
        channel.start_duty_fade(100, 0, 1000).unwrap();
        while channel.is_duty_fade_running() {}
    }
}
