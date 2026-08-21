//! Per-process file descriptor table.
//!
//! Unix programs talk to the world through small integers, so the kernel has
//! to map them onto something.  Ferrumix runs one user process at a time, so
//! a single global table is enough for now; the entries point at the devfs
//! nodes exported by [`crate::vfs`].
//!
//! ```text
//!   fd 0  stdin    keyboard buffer
//!   fd 1  stdout   console (screen and/or serial)
//!   fd 2  stderr   console
//!   fd 3+ open()   /dev/tty, /dev/ttyS0, /dev/null, /dev/zero, /dev/console
//! ```

// Descriptor helpers kept for upcoming users.
#![allow(dead_code)]

use crate::console::Target;
use crate::spinlock::IntSpinlock;

/// Number of descriptors a process may have open.
pub const MAX_FD: usize = 16;

/// What a descriptor is connected to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    /// Free slot.
    Closed,
    /// Keyboard input.
    Stdin,
    /// Console honouring the global target selection.
    Console,
    /// VGA screen only (`/dev/tty`).
    Screen,
    /// Serial port only (`/dev/ttyS0`).
    Serial,
    /// Discards writes, reads return EOF (`/dev/null`).
    Null,
    /// Reads return zero bytes (`/dev/zero`).
    Zero,
}

impl FileKind {
    /// Console target a write to this descriptor should use.
    pub fn write_target(self) -> Option<Target> {
        match self {
            FileKind::Console | FileKind::Stdin => Some(crate::console::target()),
            FileKind::Screen => Some(Target::Screen),
            FileKind::Serial => Some(Target::Serial),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Closed => "closed",
            FileKind::Stdin => "stdin",
            FileKind::Console => "console",
            FileKind::Screen => "/dev/tty",
            FileKind::Serial => "/dev/ttyS0",
            FileKind::Null => "/dev/null",
            FileKind::Zero => "/dev/zero",
        }
    }
}

struct Table {
    slots: [FileKind; MAX_FD],
}

impl Table {
    const fn new() -> Self {
        Table {
            slots: [FileKind::Closed; MAX_FD],
        }
    }
}

static TABLE: IntSpinlock<Table> = IntSpinlock::new(Table::new());

/// Reset the table and install the three standard descriptors.
pub fn reset() {
    let mut table = TABLE.lock();
    table.slots = [FileKind::Closed; MAX_FD];
    table.slots[0] = FileKind::Stdin;
    table.slots[1] = FileKind::Console;
    table.slots[2] = FileKind::Console;
}

/// Look up what a descriptor refers to.
pub fn get(fd: usize) -> FileKind {
    if fd >= MAX_FD {
        return FileKind::Closed;
    }
    TABLE.lock().slots[fd]
}

/// Resolve a device path and install it in the lowest free descriptor.
pub fn open(path: &str) -> Option<usize> {
    let name = path.strip_prefix("/dev/").unwrap_or(path);
    let kind = match name {
        "tty" | "tty0" | "vga" => FileKind::Screen,
        "ttyS0" | "serial" => FileKind::Serial,
        "console" | "stdout" => FileKind::Console,
        "null" => FileKind::Null,
        "zero" => FileKind::Zero,
        _ => return None,
    };
    // Every device we can open must exist in devfs as well.
    if crate::vfs::find_dev(name).is_none() && kind != FileKind::Console {
        return None;
    }
    let mut table = TABLE.lock();
    for fd in 0..MAX_FD {
        if table.slots[fd] == FileKind::Closed {
            table.slots[fd] = kind;
            return Some(fd);
        }
    }
    None
}

/// Release a descriptor.  Returns false if it was not open.
pub fn close(fd: usize) -> bool {
    if fd >= MAX_FD {
        return false;
    }
    let mut table = TABLE.lock();
    if table.slots[fd] == FileKind::Closed {
        return false;
    }
    table.slots[fd] = FileKind::Closed;
    true
}

/// Number of descriptors currently open.
pub fn open_count() -> usize {
    let table = TABLE.lock();
    table
        .slots
        .iter()
        .filter(|kind| **kind != FileKind::Closed)
        .count()
}
