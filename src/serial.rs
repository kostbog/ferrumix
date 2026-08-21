//! COM1 serial port (0x3F8) driver.
//!
//! This is the "serial" half of the console: it is what a headless QEMU
//! (`-serial stdio -display none`) shows, and therefore what the boot tests
//! in CI look at.  It implements [`core::fmt::Write`] and provides the
//! `serial_print!` / `serial_println!` macros for output that must bypass the
//! console target selection (panics, low level tracing).

use crate::port;
use crate::spinlock::IntSpinlock;
use core::fmt;

const COM1: u16 = 0x3f8;

pub struct SerialWriter {}

impl SerialWriter {
    pub const fn new() -> Self {
        SerialWriter {}
    }

    /// Send one byte, translating `\n` into CRLF so terminals behave.
    pub fn write_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.send(b'\r');
        }
        self.send(byte);
    }

    fn send(&mut self, byte: u8) {
        unsafe {
            // Wait for the transmitter holding register to drain.
            while (port::inb(COM1 + 5) & 0x20) == 0 {
                core::hint::spin_loop();
            }
            port::outb(COM1, byte);
        }
    }
}

impl fmt::Write for SerialWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            match byte {
                b'\r' => {}
                _ => self.write_byte(byte),
            }
        }
        Ok(())
    }
}

pub static SERIAL: IntSpinlock<SerialWriter> = IntSpinlock::new(SerialWriter::new());

/// Program the UART: 38400 baud, 8N1, FIFOs enabled.
pub fn init() {
    unsafe {
        port::outb(COM1 + 1, 0x00); // disable interrupts
        port::outb(COM1 + 3, 0x80); // enable DLAB
        port::outb(COM1, 0x03); // divisor low  (38400 baud)
        port::outb(COM1 + 1, 0x00); // divisor high
        port::outb(COM1 + 3, 0x03); // 8 bits, no parity, one stop bit
        port::outb(COM1 + 2, 0xc7); // enable + clear FIFOs, 14 byte threshold
        port::outb(COM1 + 4, 0x0b); // IRQs enabled, RTS/DSR set
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
