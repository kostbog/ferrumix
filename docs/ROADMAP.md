# Roadmap: from kernel skeleton to a Unix clone

Current state — kernel skeleton, **console (screen/serial)**, **virtual memory
operations**, **ELF loading into ring 3**, **system calls from user space** and
an interactive shell (see README).
Below is a plan for turning it into a minimal but genuine Unix-like system.
Each item is a separate, well-scoped step. Checked items are done.

## 1. Memory and processes (ring 3)
- [x] Custom `target.json` (`x86_64-ferrumix`) instead of just `unknown-none`.
      File exists, builds with nightly + `-Zbuild-std`. Default stays
      `x86_64-unknown-none` for stable compatibility, CI builds both.
- [x] Frame allocator on top of Multiboot2 mmap (bump + free list, 4 KiB).
      `src/memory.rs` — `alloc_frame`, `free_frame`, stats, kernel exclusion.
- [x] Paging helpers: PML4 introspection, virt->phys translation, CR3 logging.
      `src/paging.rs` — base for higher-half and per-process tables.
- [x] `Process`/`Task`: pid allocator, table (64 slots), init pid 1, state.
      `src/process.rs` — Unix process model start.
- [x] Separate page tables per process: `paging::new_address_space` clones the
      kernel entries into a fresh PML4, user mappings live in PML4[1]
      (0x80_0000_0000), `destroy_address_space` frees everything on exit.
- [ ] Higher-half kernel map (e.g. `-2 GiB`): remap the kernel to
      0xFFFFFFFF80000000 and drop the low identity window.
- [ ] Context switching: `switch_to` (saving/restoring RIP/RSP and page
      tables) + a scheduler (round-robin) driven by the PIT timer.
- [x] Transition to ring 3 via `iretq` into a loaded ELF binary — `src/usermode.rs`:
      address space + stack + `argc/argv/envp/auxv`, `TSS.rsp0`, and a return
      path (`leave_user_mode`) that brings the exit status back to the kernel.

## 2. System calls
- [x] `int 0x80` handler with DPL=3 gate (fallback that will coexist with
      `syscall`/`sysenter` later). Assembly stub saves GPRs and calls Rust.
      `src/syscall.rs` — handler at `0x80`, `syscall_dispatch`.
- [x] Basic set: `exit` (0/60), `write` (1), `read` (0), `open` (2, stub),
      `close` (3, stub), `getpid` (39), `brk` (12 stub).
      `write` outputs to VGA+serial; `read` reads from keyboard buffer.
- [ ] `syscall`/`sysenter` fast path (MSR_LSTAR, STAR, etc).
- [x] Copying data between user and kernel with a page walk — `src/uaccess.rs`
      (`copy_from_user`, `copy_to_user`, `copy_str_from_user`, `check_range`),
      bad pointers return `-EFAULT` instead of faulting the kernel.
- [x] Real `open`/`close`/`read`/`write` against a per-process descriptor table
      (`src/fd.rs`) and devfs, and a `brk` that maps real frames.
- [ ] Full set: `fork`, `exec`, `wait`, `mmap`.

## 3. Virtual filesystem
- [x] VFS stub with nodes (`inode`-like) and `devfs`: `null`, `zero`, `tty`, `ttyS0`.
      `src/vfs.rs` — lists devices, `find_dev` for shell `ls` command.
- [ ] Simple in-RAM filesystem (`ramfs`/`tmpfs`) for `/`, `/bin`, `/dev`.
- [x] ELF64 loader mapping `PT_LOAD` segments into a process address space with
      per-segment permissions — `src/elf.rs`; the images currently come from
      the kernel image itself (`src/user_program.rs`) because there is no disk.
- [x] File descriptor table per process — `src/fd.rs`.
- [ ] Loading ELF files from a real filesystem.

## 4. Userland environment
- [x] **Interactive shell** (`src/shell.rs`): line editing with backspace,
      echo, command dispatch. Built-in commands: `help`, `clear`, `echo`,
      `ps`, `uptime`, `mem`, `ls`, `uname`, `whoami`, `version`, `reboot`.
      Prompt: `ferrumix> ` (colored on VGA, plain on serial).
- [x] **Keyboard driver** (`src/kb_buffer.rs`): interrupt-driven character
      buffer, proper key-down filtering, no-echo design (shell handles echo).
- [x] **Interrupt-safe spinlock** (`IntSpinlock<T>`): disables interrupts
      while held, prevents deadlocks between IRQ handlers and shell display.
- [ ] `init` (pid 1): mounts the VFS, starts the shell (currently shell runs
      directly from kernel_main).
- [ ] Utilities as separate binaries: `ls`, `cat`, `echo`, `ps`, `kill`, `mkdir`, `rm`
      (freestanding, built for `x86_64-ferrumix`).

## 5. Reliability and quality of life
- [x] Text output layer that can target the screen, the serial port or both at
      runtime — `src/console.rs` (`console` shell command, `/dev/tty`, `/dev/ttyS0`).
- [ ] Output to `fb` (framebuffer from Multiboot2) instead of text-mode VGA only.
- [ ] ACPI / Local APIC timer and multi-core CPU support.
- [ ] `stdio` over the terminal with escaping, `printf` compatibility.
- [x] Tests: boot test in QEMU asserts Unix subsystems + shell prompt;
      all builds/tests on GitHub Actions.
      `.github/workflows/ci.yml` — lint, build matrix (debug/release),
      custom target, userspace, boot tests, `make test` compat.

## Architectural decisions (for the future)
- **Process model:** classic Unix — `fork`/`exec`, a process tree, `init` as pid 1.
- **Drivers:** minimal (VGA, serial, PIC, PIT, keyboard, ATA later).
- **Build:** kernel — `no_std` + `core`; userspace — freestanding for a
  custom target, loaded by the kernel from the VFS.
- **CI as source of truth:** All verification happens in GitHub Actions,
  local `make` is convenience wrapper.
