#![no_std]
#![no_main]

use core::cell::RefCell;
use core::sync::atomic::{AtomicU8, Ordering};
use critical_section::Mutex;
use esp_backtrace as _;
use esp_hal::gpio::{Event, Level, Output};
use esp_hal::gpio::{Input, InputConfig, Io, OutputConfig, Pull};
use esp_hal::time::{Duration, Instant};
use esp_hal::{handler, ram};

esp_bootloader_esp_idf::esp_app_desc!();

static PATTERN: AtomicU8 = AtomicU8::new(0);

static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut reds = [
        Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO46, Level::Low, OutputConfig::default()),
    ];
    let mut blues = [
        Output::new(peripherals.GPIO6, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO7, Level::Low, OutputConfig::default()),
    ];

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(button_handler);

    let mut boot_button = Input::new(
        peripherals.GPIO0,
        InputConfig::default().with_pull(Pull::Up),
    );

    critical_section::with(|cs| {
        boot_button.listen(Event::FallingEdge);
        BUTTON.borrow_ref_mut(cs).replace(boot_button);
    });

    loop {
        esp_println::println!("pattern: {}", PATTERN.load(Ordering::Relaxed));
        match PATTERN.load(Ordering::Relaxed) {
            0 => flashing(&mut reds, &mut blues),
            1 => police_lights(&mut reds, &mut blues),
            _ => (),
        }
    }
}

fn flashing(reds: &mut [Output], blues: &mut [Output]) {
    reds.iter_mut().for_each(|red| red.toggle());
    blues.iter_mut().for_each(|blue| blue.toggle());
    let delay_start = Instant::now();
    while delay_start.elapsed() < Duration::from_millis(500) {}
    reds.iter_mut().for_each(|red| red.toggle());
    blues.iter_mut().for_each(|blue| blue.toggle());
    let delay_start = Instant::now();
    while delay_start.elapsed() < Duration::from_millis(500) {}
}

fn police_lights<'a>(reds: &mut [Output<'a>], blues: &mut [Output<'a>]) {
    reds.iter_mut().chain(blues.iter_mut()).for_each(|light| {
        light.toggle();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(100) {}
        light.toggle();
    });
}
#[handler]
#[ram]
fn button_handler() {
    esp_println::println!(
        "GPIO Interrupt with priority {}",
        esp_hal::xtensa_lx::interrupt::get_level()
    );
    if critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .is_interrupt_set()
    }) {
        esp_println::println!("Button pressed");
        let _ = PATTERN.try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some((current + 1) % 2)
        });
    }

    critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
    })
}
