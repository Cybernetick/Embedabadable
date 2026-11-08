use crate::CURRENT_TIME_READINGS;
use crate::ds1307_clock::DS1307Clock;
use crate::fonts::large::LARGE;
use crate::fonts::small::SMALL;
use crate::fonts::{Font, FontBitOrder, FontOrientation};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::Async;
use esp_hal::i2c::master::{I2c, I2cAddress};
use esp_println::println;

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
    loop {
        let time_reading = CURRENT_TIME_READINGS.wait().await;
        let mut bus_ = bus.lock().await;
        screen_buffer.fill(0x00);
        fill_time_pixels(&time_reading, &mut screen_buffer);
        fill_date_pixels(&time_reading, &mut screen_buffer);
        fill_temp_pixels(&mut screen_buffer);
        flush_buffer(&mut bus_, &screen_buffer).await;
        drop(bus_);
    }
}

fn fill_temp_pixels(buffer: &mut [u8; 1024]) {
    let mut characters: [char; 9] = ['0'; 9];
    let font = SMALL;
    characters[0] = char::from_digit(1u32, 10).unwrap();
    characters[1] = char::from_digit(2u32, 10).unwrap();
    characters[2] = '.';
    characters[3] = char::from_digit(5u32, 10).unwrap();
    characters[4] = 0xB0.into();
    characters[5] = ' ';
    characters[6] = char::from_digit(4u32, 10).unwrap();
    characters[7] = char::from_digit(8u32, 10).unwrap();
    characters[8] = '%';

    for (index, character) in characters.iter().enumerate() {
        draw_char(buffer, 0 + (index as i16 * font.advance as i16), 50, 1, &font, *character)
    }
}

fn fill_time_pixels(time_reading: &DS1307Clock, buff: &mut [u8; 1024]) {
    let mut characters: [char; 8] = ['0'; 8];
    let font = LARGE;
    characters[0] = char::from_digit(time_reading.hour as u32 / 10, 10).unwrap();
    characters[1] = char::from_digit(time_reading.hour as u32 % 10, 10).unwrap();
    characters[2] = ':';
    characters[3] = char::from_digit(time_reading.min as u32 / 10, 10).unwrap();
    characters[4] = char::from_digit(time_reading.min as u32 % 10, 10).unwrap();
    characters[5] = ' ';
    characters[6] = char::from_digit(time_reading.sec as u32 / 10, 10).unwrap();
    characters[7] = char::from_digit(time_reading.sec as u32 % 10, 10).unwrap();

    let mut offset = 0;
    let glyph_baseline: u8 = 38;
    for (index, character) in characters.iter().enumerate() {
        let scale_factor = match index {
            0..=4 => 2,
            _ => 1
        };
        let scaled_advance = font.advance as i16 * scale_factor as i16;
        let glyph_top_y = (glyph_baseline - 7 * scale_factor) as i16;
        draw_char(buff, offset + scaled_advance, glyph_top_y, scale_factor, &font, *character);
        offset += scaled_advance;
    }
}

fn fill_date_pixels(time_reading: &DS1307Clock, buff: &mut [u8; 1024]) {
    let mut characters: [char; 10] = ['0'; 10];
    let font = SMALL;
    characters[0] = char::from_digit(time_reading.day_of_month as u32 / 10, 10).unwrap();
    characters[1] = char::from_digit(time_reading.day_of_month as u32 % 10, 10).unwrap();
    characters[2] = '.';
    characters[3] = char::from_digit(time_reading.month as u32 / 10, 10).unwrap();
    characters[4] = char::from_digit(time_reading.month as u32 % 10, 10).unwrap();
    characters[5] = '.';
    characters[6] = char::from_digit(time_reading.year as u32 / 1000, 10).unwrap();
    characters[7] = char::from_digit((time_reading.year / 100) as u32 % 10, 10).unwrap();
    characters[8] = char::from_digit((time_reading.year / 10) as u32 % 10, 10).unwrap();
    characters[9] = char::from_digit(time_reading.year as u32 % 10, 10).unwrap();
    let text_width = calc_text_width(&font, 1, &characters);
    let text_x = (SCREEN_WIDTH - text_width) / 2;
    draw_text(buff, text_x, 0, 1, &font, &characters);
}

fn draw_text(buffer: &mut [u8; 1024], x: i16, y: i16, scale: u8, font: &Font, text: &[char]) {
    let scaled_advance = font.advance as i16 * scale as i16;
    for i in 0..text.len() {
        draw_char(
            buffer,
            x + scaled_advance * i as i16,
            y,
            scale,
            font,
            text[i],
        )
    }
}

fn draw_char(
    buffer: &mut [u8; 1024],
    start_x: i16,
    start_y: i16,
    scale: u8,
    font: &Font,
    character_to_draw: char,
) {
    let glyph_data = (font.lookup)(character_to_draw);
    let (byte_count, bit_count) = match font.orientation {
        FontOrientation::Horizontal => (font.height, font.width),
        FontOrientation::Vertical => (font.width, font.height),
    };
    for i in 0..byte_count {
        for j in 0..bit_count {
            let bit_to_check = match font.bit_order {
                FontBitOrder::MsbFirst => glyph_data[i as usize] >> (7 - j),
                FontBitOrder::LsbFirst => glyph_data[i as usize] >> j,
            };
            if bit_to_check & 1 == 1 {
                let (glyph_x, glyph_y) = match font.orientation {
                    FontOrientation::Horizontal => (j as i16, i as i16),
                    FontOrientation::Vertical => (i as i16, j as i16),
                };
                // each glyph pixel becomes a scale × scale block
                for dy in 0..scale as i16 {
                    for dx in 0..scale as i16 {
                        set_pixel(
                            buffer,
                            start_x + glyph_x * scale as i16 + dx,
                            start_y + glyph_y * scale as i16 + dy,
                        );
                    }
                }
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

fn calc_text_width(font: &Font, scale: u8, text: &[char]) -> i16 {
    let mut width = 0;
    for character in text {
        width += (font.advance * scale) as i16;
    }
    width
}
