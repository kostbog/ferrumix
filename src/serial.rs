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
            // Wait for the transmit holding register to be empty (LSR bit 5).
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

/// Initialise COM1 at 38400 8N1.
pub fn init() {
    unsafe {
        port::outb(0x3F8 + 1, 0x00); // disable interrupts
        port::outb(0x3F8 + 3, 0x80); // set DLAB
        port::outb(0x3F8 + 0, 0x03); // divisor low  (38400 @ 1.8432 MHz)
        port::outb(0x3F8 + 1, 0x00); // divisor high
        port::outb(0x3F8 + 3, 0x03); // 8 data bits, no parity, 1 stop bit
        port::outb(0x3F8 + 2, 0xC7); // enable FIFO, clear, 14-byte threshold
        port::outb(0x3F8 + 1, 0x01); // enable received-data interrupt (optional)
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
    // `fmt::Error` is not `Debug`, so we can't `.unwrap()`; ignore errors.
    let _ = SERIAL.lock().write_fmt(args);
}
