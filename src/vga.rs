//! VGA text-mode framebuffer writer (0xB8000), with a `core::fmt::Write`
//! implementation and `print!`/`println!` macros.
//!
//! Every write is mirrored to the serial port as well, so a headless QEMU
//! (`-serial stdio -display none`) still shows all kernel output.

use core::fmt;
use crate::serial;
use crate::spinlock::Spinlock;

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

#[repr(C)]
struct ScreenChar {
    ascii: u8,
    color: u8,
}

const WIDTH: usize = 80;
const HEIGHT: usize = 25;
const VGA_ADDR: usize = 0xb8000;

pub struct Writer {
    col: usize,
    row: usize,
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

    pub fn clear(&mut self) {
        let blank = ScreenChar {
            ascii: b' ',
            color: self.color_code(),
        };
        for i in 0..WIDTH * HEIGHT {
            unsafe {
                let p = (VGA_ADDR as *mut ScreenChar).add(i);
                core::ptr::write_volatile(p, blank);
            }
        }
        self.col = 0;
        self.row = 0;
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
            let p = VGA_ADDR as *mut ScreenChar;
            for i in 0..WIDTH * (HEIGHT - 1) {
                let c = core::ptr::read_volatile(p.add(i + WIDTH));
                core::ptr::write_volatile(p.add(i), c);
            }
            let blank = ScreenChar {
                ascii: b' ',
                color: self.color_code(),
            };
            for i in (WIDTH * (HEIGHT - 1))..(WIDTH * HEIGHT) {
                core::ptr::write_volatile(p.add(i), blank);
            }
        }
        self.row = HEIGHT - 1;
        self.col = 0;
    }

    pub fn write_byte(&mut self, b: u8) {
        match b {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            b'\t' => {
                for _ in 0..4 {
                    self.write_byte(b' ');
                }
            }
            0x20..=0x7e => {
                unsafe {
                    let p = (VGA_ADDR as *mut ScreenChar).add(self.row * WIDTH + self.col);
                    core::ptr::write_volatile(
                        p,
                        ScreenChar {
                            ascii: b,
                            color: self.color_code(),
                        },
                    );
                }
                self.col += 1;
                if self.col >= WIDTH {
                    self.newline();
                }
            }
            _ => self.write_byte(0xfe),
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for b in s.bytes() {
            self.write_byte(b);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub static WRITER: Spinlock<Writer> = Spinlock::new(Writer::new());

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ({
        $crate::vga::_print(format_args!($($arg)*));
        $crate::print!("\n");
    });
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    // `fmt::Error` is not `Debug`, so we can't `.unwrap()` the result; a write
    // failure here is non-fatal for the kernel, so we simply ignore it.
    let _ = WRITER.lock().write_fmt(args);
    // Mirror to serial so headless runs are observable.
    let _ = serial::SERIAL.lock().write_fmt(args);
}
