#![no_std]
#![no_main]

use core::cell::RefCell;
use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::gpio::{DriveMode, Input, InputConfig, Io};
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::ledc::{channel, timer, LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::peripherals::ADC1;
use esp_hal::time::Rate;
use esp_hal::{handler, ram};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;

esp_bootloader_esp_idf::esp_app_desc!();

#[derive(Clone, Copy, PartialEq)]
enum AlarmState {
    Off,
    On,
    Triggered,
}

static ALARM_STATE: Mutex<RefCell<AlarmState>> = Mutex::new(RefCell::new(AlarmState::Off));
static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timer = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timer.timer0, sw_interrupt.software_interrupt0);

    // --- Button + interrupt ---
    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(interrupt_handler);

    let mut alarm_button = Input::new(peripherals.GPIO5, InputConfig::default());
    critical_section::with(|cs| {
        alarm_button.listen(esp_hal::gpio::Event::FallingEdge);
        BUTTON.borrow_ref_mut(cs).replace(alarm_button);
    });

    // --- ADC ---
    let mut adc_config = AdcConfig::<ADC1>::new();
    let mut adc_pin = adc_config.enable_pin(peripherals.GPIO7, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    // --- LEDC ---
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut ls_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    ls_timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty5Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(24),
        })
        .unwrap();
    let mut led_channel = ledc.channel::<LowSpeed>(channel::Number::Channel0, peripherals.GPIO6);
    led_channel
        .configure(channel::config::Config {
            timer: &ls_timer,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

    embassy_futures::join::join(
        async {
            loop {
                if matches!(
                    critical_section::with(|cs| *ALARM_STATE.borrow_ref(cs)),
                    AlarmState::On
                ) {
                    let raw = loop {
                        if let Ok(v) = adc.read_oneshot(&mut adc_pin) {
                            break v;
                        }
                    };
                    let voltage = raw as f32 * 2500.0 / 4095.0;
                    esp_println::println!("Voltage: {:.0}mv (raw: {})", voltage, raw);

                    if raw < 3500 {
                        critical_section::with(|cs| {
                            *ALARM_STATE.borrow_ref_mut(cs) = AlarmState::Triggered;
                        });
                    }

                }
                Timer::after_millis(200).await;
            }
        },
        async {
            loop {
                match critical_section::with(|cs| *ALARM_STATE.borrow_ref(cs)) {
                    AlarmState::Off => {
                        led_channel.set_duty(0);
                        Timer::after_millis(100).await;
                    }
                    AlarmState::On => {
                        led_channel.start_duty_fade(0, 100, 1000).unwrap();
                        while led_channel.is_duty_fade_running() {
                            Timer::after_millis(10).await;
                        }
                        led_channel.start_duty_fade(100, 0, 1000).unwrap();
                        while led_channel.is_duty_fade_running() {
                            Timer::after_millis(10).await;
                        }
                    }
                    AlarmState::Triggered => {
                        led_channel.set_duty(100);
                        Timer::after_millis(100).await;
                        led_channel.set_duty(0);
                        Timer::after_millis(100).await;
                    }
                }
            }
        },
    )
    .await;
}

#[handler]
#[ram]
fn interrupt_handler() {
    if critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .is_interrupt_set()
    }) {
        critical_section::with(|cs| {
            let mut state = ALARM_STATE.borrow_ref_mut(cs);
            *state = match *state {
                AlarmState::Off => AlarmState::On,
                AlarmState::On => AlarmState::Off,
                AlarmState::Triggered => AlarmState::Off,
            };
        });
        esp_println::println!("Button pressed");
    }

    critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
    });
}
