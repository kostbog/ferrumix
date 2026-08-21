//! The Unix process table.
//!
//! There is no scheduler yet, so the table mostly records what exists: `init`
//! (pid 1, the kernel shell) plus every user process the ELF loader starts.
//! Each entry remembers the address space (CR3) and the ELF entry point, which
//! is what a future `switch_to`/`fork` will need.

use crate::spinlock::IntSpinlock;
use core::sync::atomic::{AtomicU64, Ordering};

const MAX_PROCESSES: usize = 64;
const NAME_LEN: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
#[allow(dead_code)]
pub enum ProcessState {
    Unused = 0,
    Runnable = 1,
    Running = 2,
    Sleeping = 3,
    Zombie = 4,
}

impl ProcessState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProcessState::Unused => "unused",
            ProcessState::Runnable => "runnable",
            ProcessState::Running => "running",
            ProcessState::Sleeping => "sleeping",
            ProcessState::Zombie => "zombie",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Process {
    pub pid: u64,
    pub ppid: u64,
    pub state: ProcessState,
    /// Physical address of the process PML4 (0 for the kernel itself).
    pub root: u64,
    /// ELF entry point.
    pub entry: u64,
    /// Status passed to `exit`, once the process is a zombie.
    pub exit_code: i64,
    name: [u8; NAME_LEN],
    name_len: usize,
}

impl Process {
    /// An unused table slot (also handy as a `snapshot` buffer filler).
    pub const fn empty() -> Self {
        Process {
            pid: 0,
            ppid: 0,
            state: ProcessState::Unused,
            root: 0,
            entry: 0,
            exit_code: 0,
            name: [0; NAME_LEN],
            name_len: 0,
        }
    }

    /// Process name (`comm` in Unix terms).
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

struct ProcessTable {
    procs: [Process; MAX_PROCESSES],
}

impl ProcessTable {
    const fn new() -> Self {
        ProcessTable {
            procs: [Process::empty(); MAX_PROCESSES],
        }
    }
}

static PROCESS_TABLE: IntSpinlock<ProcessTable> = IntSpinlock::new(ProcessTable::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static CURRENT_PID: AtomicU64 = AtomicU64::new(1);

fn store_name(dst: &mut [u8; NAME_LEN], name: &str) -> usize {
    let bytes = name.as_bytes();
    let len = core::cmp::min(bytes.len(), NAME_LEN);
    dst[..len].copy_from_slice(&bytes[..len]);
    len
}

/// Allocate the next free pid.
pub fn alloc_pid() -> u64 {
    NEXT_PID.fetch_add(1, Ordering::Relaxed)
}

/// Create `init` (pid 1): the kernel itself, running the shell.
pub fn init() {
    let pid = alloc_pid();
    let mut table = PROCESS_TABLE.lock();
    let mut name = [0u8; NAME_LEN];
    let name_len = store_name(&mut name, "init");
    table.procs[0] = Process {
        pid,
        ppid: 0,
        state: ProcessState::Running,
        root: crate::paging::active_root(),
        entry: 0,
        exit_code: 0,
        name,
        name_len,
    };
    drop(table);
    CURRENT_PID.store(pid, Ordering::Relaxed);
    crate::serial_println!("process: init is pid {} ({} slots)", pid, MAX_PROCESSES);
}

/// Add a process to the table.  Returns its pid.
pub fn create(name: &str, root: u64, entry: u64) -> Option<u64> {
    let pid = alloc_pid();
    let ppid = current_pid();
    let mut table = PROCESS_TABLE.lock();
    for slot in table.procs.iter_mut() {
        if slot.state == ProcessState::Unused {
            let mut buf = [0u8; NAME_LEN];
            let name_len = store_name(&mut buf, name);
            *slot = Process {
                pid,
                ppid,
                state: ProcessState::Runnable,
                root,
                entry,
                exit_code: 0,
                name: buf,
                name_len,
            };
            return Some(pid);
        }
    }
    None
}

fn with_process<F: FnMut(&mut Process)>(pid: u64, mut action: F) -> bool {
    let mut table = PROCESS_TABLE.lock();
    for slot in table.procs.iter_mut() {
        if slot.state != ProcessState::Unused && slot.pid == pid {
            action(slot);
            return true;
        }
    }
    false
}

/// Change the state of a process.
pub fn set_state(pid: u64, state: ProcessState) -> bool {
    with_process(pid, |slot| slot.state = state)
}

/// Record the exit status and turn the process into a zombie.
pub fn set_exit(pid: u64, status: i64) -> bool {
    with_process(pid, |slot| {
        slot.exit_code = status;
        slot.state = ProcessState::Zombie;
    })
}

/// Remove a zombie from the table (what `wait()` will eventually do).
pub fn reap(pid: u64) -> bool {
    let mut table = PROCESS_TABLE.lock();
    for slot in table.procs.iter_mut() {
        if slot.state == ProcessState::Zombie && slot.pid == pid {
            *slot = Process::empty();
            return true;
        }
    }
    false
}

/// Pid of the process the kernel is currently running on behalf of.
pub fn current_pid() -> u64 {
    let user = crate::usermode::current_pid();
    if user != 0 {
        return user;
    }
    CURRENT_PID.load(Ordering::Relaxed)
}

/// Number of live entries in the table.
pub fn process_count() -> usize {
    let table = PROCESS_TABLE.lock();
    table
        .procs
        .iter()
        .filter(|p| p.state != ProcessState::Unused)
        .count()
}

/// Snapshot of the table, for `ps`.
pub fn snapshot(out: &mut [Process]) -> usize {
    let table = PROCESS_TABLE.lock();
    let mut count = 0;
    for slot in table.procs.iter() {
        if slot.state != ProcessState::Unused && count < out.len() {
            out[count] = *slot;
            count += 1;
        }
    }
    count
}
