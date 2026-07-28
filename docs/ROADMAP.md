# Roadmap: from kernel skeleton to a Unix clone

Current state — kernel skeleton + first real Unix step (see README). Below is a
plan for turning it into a minimal but genuine Unix-like system.
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
- [ ] Higher-half kernel map (e.g. `-2 GiB`), separate page tables per process
      (next: actually remap kernel to 0xFFFFFFFF80000000, keep low identity).
- [ ] Context switching: `switch_to` (saving/restoring RIP/RSP and page
      tables) + a scheduler (round-robin) driven by the PIT timer.
- [ ] Transition to ring 3 via `iret` into a loaded ELF userspace binary.
      GDT already has user code/data (0x18/0x20) DPL3 and TSS.rsp0 ready.

## 2. System calls
- [x] `int 0x80` handler with DPL=3 gate (fallback that will coexist with
      `syscall`/`sysenter` later). Assembly stub saves GPRs and calls Rust.
      `src/syscall.rs` — handler at `0x80`, `syscall_dispatch`.
- [x] Basic set: `exit` (0/60), `write` (1), `getpid` (39), `brk` (12 stub).
      Pointer validation (non-null, length cap), raw VGA+serial output.
      Self-test via inline `int 0x80` in `kernel_main`.
- [ ] `syscall`/`sysenter` fast path (MSR_LSTAR, STAR, etc).
- [ ] Full set: `read`, `fork`, `exec`, `wait`, `open`/`close`, `mmap`.
- [ ] Copying data between user/kernel (copy_from_user / copy_to_user with page walk).

## 3. Virtual filesystem
- [x] VFS stub with nodes (`inode`-like) and `devfs`: `null`, `zero`, `tty`, `ttyS0`.
      `src/vfs.rs` — lists devices, will back `write` and future `open`.
- [ ] Simple in-RAM filesystem (`ramfs`/`tmpfs`) for `/`, `/bin`, `/dev`.
- [ ] Loading ELF files from the VFS into a process's address space.
- [ ] File descriptor table per process.

## 4. Userland environment
- [ ] `init` (pid 1): mounts the VFS, starts the shell.
- [ ] Shell (`sh`): line parsing, `cd`, `echo`, pipes (`|`), redirections.
- [ ] Utilities: `ls`, `cat`, `echo`, `ps`, `kill`, `mkdir`, `rm`
      (freestanding, built for `x86_64-ferrumix`).

## 5. Reliability and quality of life
- [ ] Output to `fb` (framebuffer from Multiboot2) instead of text-mode VGA only.
- [ ] ACPI / Local APIC timer and multi-core CPU support.
- [ ] `stdio` over the terminal with escaping, `printf` compatibility.
- [x] Tests: boot test in QEMU asserts Unix subsystems; all builds/tests on GitHub Actions.
      `.github/workflows/ci.yml` — lint, build matrix (debug/release),
      custom target, userspace, boot tests, `make test` compat. Artifacts uploaded.

## Architectural decisions (for the future)
- **Process model:** classic Unix — `fork`/`exec`, a process tree, `init` as pid 1.
- **Drivers:** minimal (VGA, serial, PIC, PIT, keyboard, ATA later).
- **Build:** kernel — `no_std` + `core`; userspace — freestanding for a
  custom target, loaded by the kernel from the VFS.
- **CI as source of truth:** All verification happens in GitHub Actions,
  local `make` is convenience wrapper.
