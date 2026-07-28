//! COM1 serial port (0x3F8) writer, used both for debug output and as the
//! console mirror in headless QEMU. Implements `core::fmt::Write`.

use core::fmt;
use crate::port;
use crate::spinlock::Spinlock;

pub struct SerialWriter {}

impl SerialWriter {
    pub const fn new() -> Self {
        SerialWriter {}
    }

    fn send(&mut self, b: u8) {
        unsafe {
            while (port::inb(0x3F8 + 5) & 0x20) == 0 {
                core::hint::spin_loop();
            }
            port::outb(0x3F8, b);
        }
    }

    pub fn write_byte(&mut self, b: u8) {
        unsafe {
            while (port::inb(0x3F8 + 5) & 0x20) == 0 {
                core::hint::spin_loop();
            }
            port::outb(0x3F8, b);
        }
    }
}

impl fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            match b {
                b'\n' => {
                    self.send(b'\r');
                    self.send(b'\n');
                }
                b'\r' => {}
                _ => self.send(b),
            }
        }
        Ok(())
    }
}

pub static SERIAL: Spinlock<SerialWriter> = Spinlock::new(SerialWriter::new());

pub fn init() {
    unsafe {
        port::outb(0x3F8 + 1, 0x00);
        port::outb(0x3F8 + 3, 0x80);
        port::outb(0x3F8 + 0, 0x03);
        port::outb(0x3F8 + 1, 0x00);
        port::outb(0x3F8 + 3, 0x03);
        port::outb(0x3F8 + 2, 0xC7);
        port::outb(0x3F8 + 1, 0x01);
    }
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_serial_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ({
        $crate::serial::_serial_print(format_args!($($arg)*));
        $crate::serial_print!("\n");
    });
}

#[doc(hidden)]
pub fn _serial_print(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = SERIAL.lock().write_fmt(args);
}
