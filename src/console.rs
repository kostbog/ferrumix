//! Console — text output to the screen and/or the serial port.
//!
//! Everything the kernel or a user program prints goes through this single
//! entry point:
//!
//! ```text
//!   print!()/println!()  --+
//!   sys_write(fd, ...)   --+--> console --+--> VGA text framebuffer (0xB8000)
//!   shell echo           --+              +--> COM1 serial port (0x3F8)
//! ```
//!
//! Which sink is used is selected at runtime with [`set_target`] (shell
//! command `console screen|serial|both`), and an individual write can always
//! be forced to one specific sink with [`write_bytes_to`] — that is how the
//! `/dev/tty` (screen) and `/dev/ttyS0` (serial) devices are implemented.

// Console helpers kept for upcoming users.
#![allow(dead_code)]

use crate::serial;
use crate::vga;
use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};

/// Where console output is sent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Target {
    /// VGA text framebuffer only.
    Screen = 0,
    /// COM1 serial port only.
    Serial = 1,
    /// Both sinks (the default, so a headless QEMU sees everything).
    Both = 2,
}

impl Target {
    /// Human readable name, used by the shell and by boot logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Screen => "screen",
            Target::Serial => "serial",
            Target::Both => "both",
        }
    }

    /// Parse a target name (`screen`, `serial`, `both` and common aliases).
    pub fn parse(name: &str) -> Option<Target> {
        match name {
            "screen" | "vga" | "tty" | "console" => Some(Target::Screen),
            "serial" | "com1" | "ttyS0" | "uart" => Some(Target::Serial),
            "both" | "all" => Some(Target::Both),
            _ => None,
        }
    }

    fn wants_screen(self) -> bool {
        matches!(self, Target::Screen | Target::Both)
    }

    fn wants_serial(self) -> bool {
        matches!(self, Target::Serial | Target::Both)
    }
}

static TARGET: AtomicU8 = AtomicU8::new(Target::Both as u8);

/// Select where subsequent console output goes.
pub fn set_target(target: Target) {
    TARGET.store(target as u8, Ordering::Relaxed);
}

/// The currently selected output target.
pub fn target() -> Target {
    match TARGET.load(Ordering::Relaxed) {
        0 => Target::Screen,
        1 => Target::Serial,
        _ => Target::Both,
    }
}

/// Write raw bytes to one specific target, ignoring the global selection.
pub fn write_bytes_to(target: Target, bytes: &[u8]) {
    if target.wants_screen() {
        let mut screen = vga::WRITER.lock();
        for &byte in bytes {
            screen.write_byte(byte);
        }
    }
    if target.wants_serial() {
        let mut uart = serial::SERIAL.lock();
        for &byte in bytes {
            uart.write_byte(byte);
        }
    }
}

/// Write raw bytes to the currently selected target(s).
pub fn write_bytes(bytes: &[u8]) {
    write_bytes_to(target(), bytes);
}

/// Write a string to the currently selected target(s).
pub fn write_str(text: &str) {
    write_bytes(text.as_bytes());
}

/// A [`fmt::Write`] adapter so `write!`/`format_args!` can target the console.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        write_bytes(text.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    // A failing console write is not fatal for the kernel, so the result is
    // deliberately ignored (`fmt::Error` is not even `Debug`).
    let _ = Console.write_fmt(args);
}

/// Print to the console (screen and/or serial, see [`set_target`]).
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

/// Print a line to the console (screen and/or serial, see [`set_target`]).
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ({
        $crate::console::_print(format_args!($($arg)*));
        $crate::print!("\n");
    });
}

/// Report the console configuration on boot.
pub fn init() {
    crate::println!(
        "console: text output ready (target: {}, VGA 80x25 text mode + COM1 0x3f8)",
        target().as_str()
    );
}
