//! Minimal Unix process table — step toward `fork`/`exec`.

use crate::spinlock::Spinlock;
use core::sync::atomic::{AtomicU64, Ordering};

const MAX_PROCESSES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ProcessState {
    Unused = 0,
    Runnable = 1,
    Running = 2,
    Sleeping = 3,
    Zombie = 4,
}

#[derive(Clone, Copy)]
pub struct Process {
    pub pid: u64,
    pub ppid: u64,
    pub state: ProcessState,
    pub kernel_stack_top: u64,
}

impl Process {
    const fn empty() -> Self {
        Process {
            pid: 0,
            ppid: 0,
            state: ProcessState::Unused,
            kernel_stack_top: 0,
        }
    }
}

struct ProcessTable {
    procs: [Process; MAX_PROCESSES],
    next_pid: AtomicU64,
}

impl ProcessTable {
    const fn new() -> Self {
        ProcessTable {
            procs: [Process::empty(); MAX_PROCESSES],
            next_pid: AtomicU64::new(1),
        }
    }

    fn alloc_pid(&self) -> u64 {
        self.next_pid.fetch_add(1, Ordering::Relaxed)
    }

    fn create_init(&mut self) -> Option<u64> {
        let pid = self.alloc_pid();
        for p in &mut self.procs {
            if p.state == ProcessState::Unused {
                *p = Process {
                    pid,
                    ppid: 0,
                    state: ProcessState::Runnable,
                    kernel_stack_top: 0,
                };
                return Some(pid);
            }
        }
        None
    }

    fn count(&self) -> usize {
        self.procs.iter().filter(|p| p.state != ProcessState::Unused).count()
    }
}

static PROCESS_TABLE: Spinlock<ProcessTable> = Spinlock::new(ProcessTable::new());
static CURRENT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init() {
    let mut table = PROCESS_TABLE.lock();
    if let Some(pid) = table.create_init() {
        CURRENT_PID.store(pid, Ordering::Relaxed);
        crate::serial::serial_println!("process: init created pid={} (table size={})", pid, MAX_PROCESSES);
    } else {
        crate::serial::serial_println!("process: FAILED to create init");
    }
}

pub fn current_pid() -> u64 {
    CURRENT_PID.load(Ordering::Relaxed)
}

pub fn process_count() -> usize {
    PROCESS_TABLE.lock().count()
}

pub fn alloc_pid() -> u64 {
    PROCESS_TABLE.lock().alloc_pid()
}
