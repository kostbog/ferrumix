//! Ring 3: building a user address space, entering it and coming back.
//!
//! Running a program means:
//!
//! 1. create a private address space that still contains the kernel mappings
//!    ([`crate::paging::new_address_space`]),
//! 2. load the ELF image into it ([`crate::elf::load`]),
//! 3. map a user stack and lay out the initial `argc/argv/envp/auxv` frame,
//! 4. point `TSS.rsp0` at a kernel stack so traps have somewhere to land,
//! 5. `iretq` into ring 3 with the user code/data selectors,
//! 6. come back when the program calls `exit` (or faults) and tear the
//!    address space down again.
//!
//! Steps 5 and 6 are the two assembly routines below: `enter_user_mode` saves
//! the kernel context, switches privilege level and never "returns" the usual
//! way — the return happens from `leave_user_mode`, which restores the saved
//! kernel stack and hands the exit status back as the return value.

use crate::elf;
use crate::fd;
use crate::gdt;
use crate::paging;
use crate::process;
use crate::uaccess;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// First byte of the user half of the address space (PML4 slot 1).
pub const USER_SPACE_BASE: u64 = 0x0000_0080_0000_0000;
/// Where the embedded programs are linked to load.
pub const USER_TEXT_BASE: u64 = USER_SPACE_BASE + 0x1000;
/// Top of the user stack (grows down from here).
pub const USER_STACK_TOP: u64 = USER_SPACE_BASE + 0x0080_0000;
/// Size of the user stack, in 4 KiB pages.
pub const USER_STACK_PAGES: u64 = 16;
/// Exit status reported for a process killed by a fault (as in `128 + SIGSEGV`).
pub const EXIT_FAULT: i64 = 139;

/// Why a program could not be started.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExecError {
    /// A user process is already running (no scheduler yet).
    Busy,
    /// The address space or the stack could not be created.
    Vm(paging::VmError),
    /// The image is not a usable ELF executable.
    Elf(elf::ElfError),
    /// The initial stack frame could not be written.
    Stack(uaccess::UserFault),
    /// The process table is full.
    NoProcessSlot,
}

core::arch::global_asm!(
    r#"
.section .data
.align 8
kernel_return_rsp:
    .quad 0

.section .text

// u64 enter_user_mode(u64 entry /* rdi */, u64 user_rsp /* rsi */)
//
// Saves the callee-saved registers and the kernel stack pointer, loads the
// ring-3 data selectors and drops to user mode with iretq.  It "returns" only
// through leave_user_mode, with the exit status in rax.
.global enter_user_mode
enter_user_mode:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rip + kernel_return_rsp], rsp

    mov ax, 0x1b                 // user data selector, RPL 3
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    push 0x1b                    // ss
    push rsi                     // rsp
    push 0x202                   // rflags: IF set
    push 0x23                    // cs: user code selector, RPL 3
    push rdi                     // rip
    iretq

// void leave_user_mode(u64 status /* rdi */) -> returns from enter_user_mode
.global leave_user_mode
leave_user_mode:
    mov rsp, [rip + kernel_return_rsp]
    mov ax, 0x10                 // back to the kernel data selectors
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    mov rax, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    sti                          // the trap gate cleared IF on the way in
    ret
"#
);

extern "C" {
    fn enter_user_mode(entry: u64, user_rsp: u64) -> u64;
    fn leave_user_mode(status: u64) -> !;
}

static IN_USER: AtomicBool = AtomicBool::new(false);
static CURRENT_ROOT: AtomicU64 = AtomicU64::new(0);
static CURRENT_BRK: AtomicU64 = AtomicU64::new(0);
static CURRENT_PID: AtomicU64 = AtomicU64::new(0);

/// True while a ring-3 process is running (or trapped into the kernel).
pub fn in_user_mode() -> bool {
    IN_USER.load(Ordering::Relaxed)
}

/// Address space of the running user process (or the active one otherwise).
pub fn current_root() -> u64 {
    let root = CURRENT_ROOT.load(Ordering::Relaxed);
    if root == 0 {
        paging::active_root()
    } else {
        root
    }
}

/// PID of the running user process.
pub fn current_pid() -> u64 {
    CURRENT_PID.load(Ordering::Relaxed)
}

fn write_u64(root: u64, addr: u64, value: u64) -> Result<(), uaccess::UserFault> {
    unsafe { uaccess::write_phys_backed(root, addr, &value.to_le_bytes()) }
}

/// Map the user stack and build the initial `argc/argv/envp/auxv` frame.
///
/// ```text
///   USER_STACK_TOP  -> [ "hello\0" program name ]
///                      [ AT_NULL   0            ]
///                      [ envp      NULL         ]
///                      [ argv      NULL         ]
///                      [ argv[0]   name pointer ]
///   rsp             -> [ argc      1            ]
/// ```
fn setup_stack(root: u64, name: &str) -> Result<u64, ExecError> {
    let flags = paging::ENTRY_PRESENT
        | paging::ENTRY_WRITABLE
        | paging::ENTRY_USER
        | if paging::nx_enabled() {
            paging::ENTRY_NX
        } else {
            0
        };
    let bottom = USER_STACK_TOP - USER_STACK_PAGES * paging::PAGE_SIZE;
    unsafe { paging::map_alloc(root, bottom, USER_STACK_PAGES, flags) }.map_err(ExecError::Vm)?;

    // Program name string, right below the top of the stack.
    let name_bytes = name.as_bytes();
    let name_len = core::cmp::min(name_bytes.len(), 31);
    let name_addr = USER_STACK_TOP - 32;
    unsafe { uaccess::write_phys_backed(root, name_addr, &name_bytes[..name_len]) }
        .map_err(ExecError::Stack)?;
    unsafe { uaccess::write_phys_backed(root, name_addr + name_len as u64, &[0u8]) }
        .map_err(ExecError::Stack)?;

    let rsp = USER_STACK_TOP - 128;
    write_u64(root, rsp, 1).map_err(ExecError::Stack)?; // argc
    write_u64(root, rsp + 8, name_addr).map_err(ExecError::Stack)?; // argv[0]
    write_u64(root, rsp + 16, 0).map_err(ExecError::Stack)?; // argv NULL
    write_u64(root, rsp + 24, 0).map_err(ExecError::Stack)?; // envp NULL
    write_u64(root, rsp + 32, 0).map_err(ExecError::Stack)?; // auxv AT_NULL
    write_u64(root, rsp + 40, 0).map_err(ExecError::Stack)?;
    Ok(rsp)
}

/// Load an ELF image and run it in ring 3, returning its exit status.
pub fn exec(name: &'static str, image: &[u8]) -> Result<i64, ExecError> {
    if in_user_mode() {
        return Err(ExecError::Busy);
    }

    let root = paging::new_address_space().map_err(ExecError::Vm)?;
    crate::serial::trace(b'N');

    let loaded = match unsafe { elf::load(root, image) } {
        Ok(loaded) => loaded,
        Err(err) => {
            unsafe { paging::destroy_address_space(root) };
            return Err(ExecError::Elf(err));
        }
    };

    crate::serial::trace(b'L');
    let user_rsp = match setup_stack(root, name) {
        Ok(rsp) => rsp,
        Err(err) => {
            unsafe { paging::destroy_address_space(root) };
            return Err(err);
        }
    };

    crate::serial::trace(b'S');
    let pid = match process::create(name, root, loaded.entry) {
        Some(pid) => pid,
        None => {
            unsafe { paging::destroy_address_space(root) };
            return Err(ExecError::NoProcessSlot);
        }
    };

    crate::serial::trace(b'P');
    crate::println!(
        "elf: {} loaded - entry {:#x}, {} segment(s), {} page(s), brk {:#x}",
        name,
        loaded.entry,
        loaded.segments,
        loaded.pages,
        loaded.brk
    );
    crate::println!(
        "usermode: pid {} entering ring 3 at {:#x}, stack {:#x} (cr3 {:#x})",
        pid,
        loaded.entry,
        user_rsp,
        root
    );

    fd::reset();
    gdt::set_kernel_stack(gdt::kernel_stack_top());
    CURRENT_ROOT.store(root, Ordering::Relaxed);
    CURRENT_BRK.store(loaded.brk, Ordering::Relaxed);
    CURRENT_PID.store(pid, Ordering::Relaxed);
    IN_USER.store(true, Ordering::Relaxed);
    process::set_state(pid, process::ProcessState::Running);

    crate::serial::trace(b'X');
    let kernel_root = paging::active_root();
    let status = unsafe {
        paging::switch_to(root);
        let status = enter_user_mode(loaded.entry, user_rsp) as i64;
        paging::switch_to(kernel_root);
        status
    };

    crate::serial::trace(b'R');
    IN_USER.store(false, Ordering::Relaxed);
    CURRENT_ROOT.store(0, Ordering::Relaxed);
    CURRENT_PID.store(0, Ordering::Relaxed);
    process::set_exit(pid, status);
    let freed = unsafe { paging::destroy_address_space(root) };
    process::reap(pid);
    crate::println!(
        "usermode: pid {} exited with status {} ({} user frames freed)",
        pid,
        status,
        freed
    );
    Ok(status)
}

/// Terminate the running user process with `status` (the `exit` system call).
pub fn exit_current(status: i64) -> ! {
    if in_user_mode() {
        unsafe { leave_user_mode(status as u64) };
    }
    crate::println!("exit({}) outside of user mode — halting", status);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Kill the running user process after a fault it cannot recover from.
pub fn abort_current(reason: &str) -> ! {
    if in_user_mode() {
        crate::println!(
            "usermode: pid {} killed by {} (status {})",
            current_pid(),
            reason,
            EXIT_FAULT
        );
        unsafe { leave_user_mode(EXIT_FAULT as u64) };
    }
    crate::println!("kernel fault ({}) — halting", reason);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// The `brk` system call: query or move the end of the process data segment.
pub fn brk(request: u64) -> u64 {
    let current = CURRENT_BRK.load(Ordering::Relaxed);
    if request == 0 || !in_user_mode() {
        return current;
    }
    let heap_limit = USER_STACK_TOP - USER_STACK_PAGES * paging::PAGE_SIZE;
    if !(USER_TEXT_BASE..heap_limit).contains(&request) {
        return current;
    }

    let root = current_root();
    let page = paging::PAGE_SIZE;
    let old_end = (current + page - 1) & !(page - 1);
    let new_end = (request + page - 1) & !(page - 1);
    let flags = paging::ENTRY_PRESENT
        | paging::ENTRY_WRITABLE
        | paging::ENTRY_USER
        | if paging::nx_enabled() {
            paging::ENTRY_NX
        } else {
            0
        };

    if new_end > old_end {
        let pages = (new_end - old_end) / page;
        if unsafe { paging::map_alloc(root, old_end, pages, flags) }.is_err() {
            return current;
        }
    } else if new_end < old_end {
        let pages = (old_end - new_end) / page;
        unsafe { paging::unmap_alloc(root, new_end, pages) };
    }

    CURRENT_BRK.store(request, Ordering::Relaxed);
    request
}
