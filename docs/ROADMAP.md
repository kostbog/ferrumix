# Roadmap: from kernel skeleton to a Unix clone

Current state — a working "bare" kernel in ring 0 (see README). Below is a
plan for turning it into a minimal but genuine Unix-like system.
Each item is a separate, well-scoped step.

## 1. Memory and processes (ring 3)
- [ ] Custom `target.json` (`x86_64-ferrumix`) instead of just `unknown-none`.
- [ ] Higher-half kernel map (e.g. `-2 GiB`), separate page tables per
      process; frame allocator (buddy / bitmap) on top of the Multiboot2 mmap.
- [ ] `Task`/`Process`: address space, registers, kernel stack.
- [ ] Context switching: `switch_to` (saving/restoring RIP/RSP and page
      tables) + a scheduler (round-robin) driven by the PIT timer.
- [ ] Transition to ring 3 via `iret` into a loaded ELF userspace binary.

## 2. System calls
- [ ] `syscall`/`sysenter` handler (with a fallback `int 0x80`).
- [ ] Basic set: `exit`, `write`, `read`, `fork`, `exec`, `wait`,
      `open`/`close`/`read`/`write` for the VFS, `getpid`, `brk`/`mmap`.
- [ ] Copying data between user/kernel (pointer validation!).

## 3. Virtual filesystem
- [ ] VFS with nodes (`inode`-like) and operations.
- [ ] `devfs`: `tty` (our VGA/serial), `null`, `zero`.
- [ ] Simple in-RAM filesystem (`ramfs`/`tmpfs`) for `/`, `/bin`, `/dev`.
- [ ] Loading ELF files from the VFS into a process's address space.

## 4. Userland environment
- [ ] `init` (pid 1): mounts the VFS, starts the shell.
- [ ] Shell (`sh`): line parsing, `cd`, `echo`, pipes (`|`), redirections.
- [ ] Utilities: `ls`, `cat`, `echo`, `ps`, `kill`, `mkdir`, `rm`
      (freestanding, built for `x86_64-ferrumix`).

## 5. Reliability and quality of life
- [ ] Output to `fb` (framebuffer from Multiboot2) instead of text-mode
      VGA only.
- [ ] ACPI / Local APIC timer and multi-core CPU support.
- [ ] `stdio` over the terminal with escaping, `printf` compatibility.
- [ ] Tests: unit tests for memory/page ports, integration tests in QEMU.

## Architectural decisions (for the future)
- **Process model:** classic Unix — `fork`/`exec`, a process tree,
  `init` as pid 1.
- **Drivers:** minimal (VGA, serial, PIC, PIT, keyboard, ATA later).
- **Build:** kernel — `no_std` + `core`; userspace — freestanding for a
  custom target, loaded by the kernel from the VFS.
