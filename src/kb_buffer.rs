//! PS/2 keyboard decoding and the character buffer used by the shell.
//!
//! IRQ1 supplies Set 1 scancodes, including key releases and modifier keys.
//! This module keeps the decoder state and character queue under one
//! interrupt-safe lock. The shell consumes decoded bytes and remains
//! responsible for echoing and line editing.

use crate::spinlock::IntSpinlock;

const CHAR_BUF_SIZE: usize = 512;

/// Ring buffer for decoded keyboard characters.
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

struct KeyboardInput {
    chars: CharRing,
    left_shift: bool,
    right_shift: bool,
    caps_lock: bool,
    extended: bool,
}

impl KeyboardInput {
    const fn new() -> Self {
        KeyboardInput {
            chars: CharRing::new(),
            left_shift: false,
            right_shift: false,
            caps_lock: false,
            extended: false,
        }
    }

    fn handle_scancode(&mut self, scancode: u8) {
        // Extended Set 1 keys are prefixed with E0. None of them currently
        // produce shell input, but consuming the following byte prevents it
        // from being mistaken for a key on the primary map.
        if scancode == 0xE0 {
            self.extended = true;
            return;
        }
        if self.extended {
            self.extended = false;
            return;
        }

        let released = scancode & 0x80 != 0;
        let code = scancode & 0x7F;
        match code {
            0x2A => {
                self.left_shift = !released;
                return;
            }
            0x36 => {
                self.right_shift = !released;
                return;
            }
            0x3A if !released => {
                self.caps_lock = !self.caps_lock;
                return;
            }
            _ => {}
        }

        if released {
            return;
        }

        if let Some(c) = self.decode(code) {
            self.chars.push(c);
        }
    }

    fn decode(&self, code: u8) -> Option<u8> {
        let shifted = self.left_shift || self.right_shift;
        let c = match code {
            0x02 => choose(shifted, b'1', b'!'),
            0x03 => choose(shifted, b'2', b'@'),
            0x04 => choose(shifted, b'3', b'#'),
            0x05 => choose(shifted, b'4', b'$'),
            0x06 => choose(shifted, b'5', b'%'),
            0x07 => choose(shifted, b'6', b'^'),
            0x08 => choose(shifted, b'7', b'&'),
            0x09 => choose(shifted, b'8', b'*'),
            0x0A => choose(shifted, b'9', b'('),
            0x0B => choose(shifted, b'0', b')'),
            0x0C => choose(shifted, b'-', b'_'),
            0x0D => choose(shifted, b'=', b'+'),
            0x0E => 0x08,
            // Tab completion is not implemented by the shell yet.
            0x0F => return None,
            0x10 => self.letter(b'q'),
            0x11 => self.letter(b'w'),
            0x12 => self.letter(b'e'),
            0x13 => self.letter(b'r'),
            0x14 => self.letter(b't'),
            0x15 => self.letter(b'y'),
            0x16 => self.letter(b'u'),
            0x17 => self.letter(b'i'),
            0x18 => self.letter(b'o'),
            0x19 => self.letter(b'p'),
            0x1A => choose(shifted, b'[', b'{'),
            0x1B => choose(shifted, b']', b'}'),
            0x1C => b'\n',
            0x1E => self.letter(b'a'),
            0x1F => self.letter(b's'),
            0x20 => self.letter(b'd'),
            0x21 => self.letter(b'f'),
            0x22 => self.letter(b'g'),
            0x23 => self.letter(b'h'),
            0x24 => self.letter(b'j'),
            0x25 => self.letter(b'k'),
            0x26 => self.letter(b'l'),
            0x27 => choose(shifted, b';', b':'),
            0x28 => choose(shifted, b'\'', b'"'),
            0x29 => choose(shifted, b'`', b'~'),
            0x2B => choose(shifted, b'\\', b'|'),
            0x2C => self.letter(b'z'),
            0x2D => self.letter(b'x'),
            0x2E => self.letter(b'c'),
            0x2F => self.letter(b'v'),
            0x30 => self.letter(b'b'),
            0x31 => self.letter(b'n'),
            0x32 => self.letter(b'm'),
            0x33 => choose(shifted, b',', b'<'),
            0x34 => choose(shifted, b'.', b'>'),
            0x35 => choose(shifted, b'/', b'?'),
            0x39 => b' ',
            _ => return None,
        };
        Some(c)
    }

    fn letter(&self, lower: u8) -> u8 {
        if (self.left_shift || self.right_shift) ^ self.caps_lock {
            lower - b'a' + b'A'
        } else {
            lower
        }
    }
}

const fn choose(condition: bool, normal: u8, alternate: u8) -> u8 {
    if condition {
        alternate
    } else {
        normal
    }
}

static KEYBOARD: IntSpinlock<KeyboardInput> = IntSpinlock::new(KeyboardInput::new());

/// Feed one raw PS/2 Set 1 scancode from the keyboard interrupt handler.
pub fn on_scancode(scancode: u8) {
    KEYBOARD.lock().handle_scancode(scancode);
}

/// Try to read one decoded character (non-blocking).
/// Returns `None` if the buffer is empty.
pub fn try_read_char() -> Option<u8> {
    KEYBOARD.lock().chars.pop()
}

/// Check whether any decoded characters are available.
pub fn has_input() -> bool {
    !KEYBOARD.lock().chars.is_empty()
}
