//! VGA text-mode framebuffer driver (0xB8000).
//!
//! This is the "screen" half of the console: an 80x25 character grid with a
//! colour attribute per cell, line wrapping, scrolling and a hardware cursor.
//! Higher layers (see [`crate::console`]) decide whether text goes here, to
//! the serial port, or to both.

use crate::port;
use crate::spinlock::IntSpinlock;
use core::fmt;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScreenChar {
    ascii: u8,
    color: u8,
}

pub const WIDTH: usize = 80;
pub const HEIGHT: usize = 25;
const VGA_ADDR: usize = 0xb8000;

pub struct Writer {
    pub col: usize,
    pub row: usize,
    fg: Color,
    bg: Color,
}

impl Writer {
    pub const fn new() -> Self {
        Writer {
            col: 0,
            row: 0,
            fg: Color::LightGreen,
            bg: Color::Black,
        }
    }

    fn color_code(&self) -> u8 {
        (self.bg as u8) << 4 | (self.fg as u8)
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.fg = fg;
        self.bg = bg;
    }

    pub fn clear(&mut self) {
        let blank = ScreenChar {
            ascii: b' ',
            color: self.color_code(),
        };
        for i in 0..WIDTH * HEIGHT {
            unsafe {
                let cell = (VGA_ADDR as *mut ScreenChar).add(i);
                core::ptr::write_volatile(cell, blank);
            }
        }
        self.col = 0;
        self.row = 0;
        self.update_cursor();
    }

    fn newline(&mut self) {
        self.col = 0;
        if self.row + 1 < HEIGHT {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        unsafe {
            let base = VGA_ADDR as *mut ScreenChar;
            for i in 0..WIDTH * (HEIGHT - 1) {
                let cell = core::ptr::read_volatile(base.add(i + WIDTH));
                core::ptr::write_volatile(base.add(i), cell);
            }
            let blank = ScreenChar {
                ascii: b' ',
                color: self.color_code(),
            };
            for i in (WIDTH * (HEIGHT - 1))..(WIDTH * HEIGHT) {
                core::ptr::write_volatile(base.add(i), blank);
            }
        }
        self.row = HEIGHT - 1;
        self.col = 0;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            b'\t' => {
                for _ in 0..4 {
                    self.put(b' ');
                }
            }
            0x20..=0x7e => self.put(byte),
            // Anything else (control characters, UTF-8 continuation bytes)
            // becomes the code page 437 replacement glyph.  This must not go
            // back through `write_byte`: 0xfe is not printable ASCII, so that
            // would recurse forever.
            _ => self.put(0xfe),
        }
        self.update_cursor();
    }

    /// Put one glyph at the cursor and advance, wrapping at the right edge.
    fn put(&mut self, byte: u8) {
        let cell = ScreenChar {
            ascii: byte,
            color: self.color_code(),
        };
        unsafe {
            let slot = (VGA_ADDR as *mut ScreenChar).add(self.row * WIDTH + self.col);
            core::ptr::write_volatile(slot, cell);
        }
        self.col += 1;
        if self.col >= WIDTH {
            self.newline();
        }
    }

    pub fn write_string(&mut self, text: &str) {
        for byte in text.bytes() {
            self.write_byte(byte);
        }
    }

    /// Move the blinking hardware cursor to the current position.
    fn update_cursor(&self) {
        let pos = (self.row * WIDTH + self.col) as u16;
        unsafe {
            port::outb(0x3d4, 0x0f);
            port::outb(0x3d5, (pos & 0xff) as u8);
            port::outb(0x3d4, 0x0e);
            port::outb(0x3d5, (pos >> 8) as u8);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.write_string(text);
        Ok(())
    }
}

pub static WRITER: IntSpinlock<Writer> = IntSpinlock::new(Writer::new());
