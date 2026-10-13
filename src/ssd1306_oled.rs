use crate::CURRENT_TIME_READINGS;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::i2c::master::{I2c, I2cAddress};
use esp_hal::{Async, Blocking};
use esp_println::println;
use font8x8::UnicodeFonts;

const OLED_ADDRESS: u8 = 0x3C;
const SCREEN_WIDTH: i16 = 128;
const SCREEN_HEIGHT: i16 = 64;
enum Ssd1306Command {
    NoArg(u8),
    OneArg(u8, u8),
    TwoArg(u8, u8, u8),
}

const INIT_COMMANDS: [Ssd1306Command; 16] = [
    Ssd1306Command::NoArg(0xAE),        /* display off                */
    Ssd1306Command::OneArg(0xD5, 0x80), /* clock divide / osc freq    */
    Ssd1306Command::OneArg(0xA8, 0x3F), /* multiplex ratio = 64 рядки */
    Ssd1306Command::OneArg(0xD3, 0x00), /* display offset = 0         */
    Ssd1306Command::NoArg(0x40),        /* start line = 0             */
    Ssd1306Command::OneArg(0x8D, 0x14), /* charge pump ON             */
    Ssd1306Command::OneArg(0x20, 0x00), /* horizontal addressing mode */
    Ssd1306Command::NoArg(0xA1),        /* segment remap (дзеркало X) */
    Ssd1306Command::NoArg(0xC8),        /* COM scan direction (Y)     */
    Ssd1306Command::OneArg(0xDA, 0x12), /* COM pins config            */
    Ssd1306Command::OneArg(0x81, 0x9F), /* contrast                   */
    Ssd1306Command::OneArg(0xD9, 0xF1), /* pre-charge                 */
    Ssd1306Command::OneArg(0xDB, 0x30), /* VCOMH deselect             */
    Ssd1306Command::NoArg(0xA4),        /* виводити вміст RAM         */
    Ssd1306Command::NoArg(0xA6),        /* нормальний (не інверсний)  */
    Ssd1306Command::NoArg(0xAF),        /* display on                 */
];

pub async fn run_init_sequence(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) {
    let mut bus_ = bus.lock().await;
    for command in INIT_COMMANDS.iter() {
        sends_command(&mut bus_, command).await
    }
    clear_screen(&mut bus_).await;
}

async fn sends_command(bus: &mut I2c<'static, Async>, command: &Ssd1306Command) {
    match command {
        Ssd1306Command::NoArg(cmd) => bus
            .write_async(I2cAddress::from(OLED_ADDRESS), &[0x00, *cmd])
            .await
            .unwrap(),
        Ssd1306Command::OneArg(cmd, arg) => bus
            .write_async(I2cAddress::from(OLED_ADDRESS), &[0x00, *cmd, *arg])
            .await
            .unwrap(),
        Ssd1306Command::TwoArg(cmd, arg1, arg2) => bus
            .write_async(I2cAddress::from(OLED_ADDRESS), &[0x00, *cmd, *arg1, *arg2])
            .await
            .unwrap(),
    }
}
#[embassy_executor::task]
pub async fn draw_clock_time(bus: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) {
    let mut screen_buffer = [0x00; 1024];
    let fonts = font8x8::BASIC_FONTS;
    loop {
        let time_reading = CURRENT_TIME_READINGS.wait().await;
        let mut bus_ = bus.lock().await;
        screen_buffer.fill(0x00);
        let mut characters: [char; 8] = ['0'; 8];
        characters[0] = char::from_digit(time_reading.hour as u32 / 10, 10).unwrap();
        characters[1] = char::from_digit(time_reading.hour as u32 % 10, 10).unwrap();
        characters[2] = ':';
        characters[3] = char::from_digit(time_reading.min as u32 / 10, 10).unwrap();
        characters[4] = char::from_digit(time_reading.min as u32 % 10, 10).unwrap();
        characters[5] = ':';
        characters[6] = char::from_digit(time_reading.sec as u32 / 10, 10).unwrap();
        characters[7] = char::from_digit(time_reading.sec as u32 % 10, 10).unwrap();

        for i in 0..characters.len() {
            draw_char(&mut screen_buffer, 20 + 10 * i as i16, 24, &fonts.get(characters[i]).unwrap())
        }
        flush_buffer(&mut bus_, &screen_buffer).await;
        println!(
            "Time is {}:{}:{}",
            time_reading.hour, time_reading.min, time_reading.sec
        );
        drop(bus_);
    }
}

fn draw_char(buffer: &mut [u8; 1024], x: i16, y: i16, font: &[u8; 8]) {
    for i in 0..font.len() {
        for j in 0..font.len() {
            if (font[i] >> j) & 1 == 1 {
                set_pixel(buffer, x + j as i16, y + i as i16);
            }
        }
    }
}

async fn clear_screen(bus: &mut I2c<'static, Async>) {
    let column_address_command = Ssd1306Command::TwoArg(0x21, 0x00, 0x7F);
    sends_command(bus, &column_address_command).await;

    let set_page_address_command = Ssd1306Command::TwoArg(0x22, 0x00, 0x07);
    sends_command(bus, &set_page_address_command).await;

    let mut buffer = [0x00; 1025];
    buffer[0] = 0x40;
    bus.write_async(I2cAddress::from(OLED_ADDRESS), &buffer)
        .await
        .unwrap();
}

async fn flush_buffer(bus: &mut I2c<'static, Async>, screen_buffer: &[u8; 1024]) {
    let column_address_command = Ssd1306Command::TwoArg(0x21, 0x00, 0x7F);
    sends_command(bus, &column_address_command).await;

    let set_page_address_command = Ssd1306Command::TwoArg(0x22, 0x00, 0x07);
    sends_command(bus, &set_page_address_command).await;

    let mut buffer = [0x00; 1025];
    buffer[0] = 0x40;
    buffer[1..].copy_from_slice(screen_buffer);
    bus.write_async(I2cAddress::from(OLED_ADDRESS), &buffer)
        .await
        .unwrap();
}

fn set_pixel(buffer: &mut [u8; 1024], x: i16, y: i16) {
    if x < 0 || x >= SCREEN_WIDTH || y < 0 || y >= SCREEN_HEIGHT {
        return;
    }
    let index = (y / 8) as usize * SCREEN_WIDTH as usize + x as usize;
    buffer[index] |= 1 << (y % 8);
}
