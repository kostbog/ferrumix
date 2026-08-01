//! Keyboard character buffer — collects keystrokes for the shell.
//!
//! The keyboard interrupt handler pushes raw characters here.
//! The shell reads them and handles echoing, line editing, and line assembly.
//! This avoids deadlocks between the interrupt handler (which needs KB lock)
//! and the shell (which needs WRITER lock for display).

use crate::spinlock::Spinlock;

const CHAR_BUF_SIZE: usize = 512;

/// Ring buffer for raw keyboard characters.
struct CharRing {
    buf: [u8; CHAR_BUF_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl CharRing {
    const fn new() -> Self {
        CharRing {
            buf: [0; CHAR_BUF_SIZE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, c: u8) {
        if self.count < CHAR_BUF_SIZE {
            self.buf[self.head] = c;
            self.head = (self.head + 1) % CHAR_BUF_SIZE;
            self.count += 1;
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let c = self.buf[self.tail];
        self.tail = (self.tail + 1) % CHAR_BUF_SIZE;
        self.count -= 1;
        Some(c)
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

static KB: Spinlock<CharRing> = Spinlock::new(CharRing::new());

/// Called from the keyboard interrupt handler on each key-down event.
/// Simply buffers the character — NO echoing (the shell handles that).
pub fn on_key(c: char) {
    match c {
        '\n' | '\r' => KB.lock().push(b'\n'),
        '\u{8}' => KB.lock().push(0x08), // backspace
        _ if c as u32 >= 0x20 && c as u32 <= 0x7e => {
            KB.lock().push(c as u8);
        }
        _ => {} // ignore non-printable
    }
}

/// Try to read one character from the buffer (non-blocking).
/// Returns None if the buffer is empty.
pub fn try_read_char() -> Option<u8> {
    KB.lock().pop()
}

/// Check if any characters are available.
pub fn has_input() -> bool {
    !KB.lock().is_empty()
}
