# Ferrumix

![CI](https://github.com/kostbog/ferrumix/actions/workflows/ci.yml/badge.svg)

> *ferrum* (Latin for "iron") + *ix* — a Unix clone written in Rust.

Ferrumix is an **x86_64 OS kernel written from scratch**, booted by QEMU
(PVH) or GRUB (Multiboot2). It has a text console on the screen and the
serial port, a physical frame allocator, real **paging and virtual memory**
operations, per-process address spaces, an **ELF64 loader** that maps a
program and jumps to it **in ring 3**, and a **system call interface** that
user space reaches through `int 0x80` — plus an interactive shell to drive
all of it. Everything is verified by a QEMU boot test in CI.

---

## What already works

### Boot & low level
- 32→64-bit trampoline, `.bss` zeroing and long mode — `src/boot.S`
- Two boot descriptions in one image: a **PVH ELF note** (QEMU `-kernel`,
  which refuses Multiboot for 64-bit ELFs) and a **Multiboot2 header** (GRUB);
  `src/multiboot.rs` normalises `hvm_start_info`, Multiboot1 and Multiboot2
  memory maps into one structure
- GDT + TSS with ring-3 code/data segments, an IST stack for double faults and
  `rsp0` for ring3→ring0 transitions
- IDT built from **assembly interrupt stubs** (`isr_stub_0..47`, `isr_stub_128`)
  that save a full [`TrapFrame`] and call one Rust dispatcher — no unstable
  `x86-interrupt` ABI, so the kernel builds on stable Rust
- 8259 PIC (remapped), PIT timer (~100 Hz), PS/2 keyboard

### 1. Text output — screen and serial port
`src/console.rs` is the single text sink of the system:

- VGA text mode driver (80x25, colours, scrolling, hardware cursor) — `src/vga.rs`
- COM1 driver with CRLF translation — `src/serial.rs`
- a runtime **target selector**: `screen`, `serial` or `both`
  (`console` shell command), so the same `println!` can go to the monitor, to
  the serial line, or to both
- device-level access for user space: `/dev/tty` (screen), `/dev/ttyS0`
  (serial), `/dev/console`, `/dev/null`, `/dev/zero`
- `print!` / `println!` for the kernel, `serial_print!` / `serial_println!`
  when output must bypass the selection (panics, early traces)

### 2. Paging and virtual memory
`src/paging.rs` implements the operations a Unix VM needs:

| operation | function |
|-----------|----------|
| map a 4 KiB page | `map_page(root, virt, phys, flags)` |
| unmap and reclaim | `unmap_page`, `unmap_alloc` |
| map a range / fresh zeroed frames | `map_range`, `map_alloc` |
| split a 2 MiB huge page into a page table | automatic inside `map_page` |
| software page-table walk | `translate_in`, `entry_in`, `dump_walk` |
| new / destroyed address space | `new_address_space`, `destroy_address_space` |
| switch address space, TLB | `switch_to`, `flush`, `flush_all` |
| W^X support | `ENTRY_NX` after enabling `EFER.NXE` |

A boot self-test maps a scratch page, writes through the virtual address,
reads the value back through the physical one, unmaps it and checks that the
translation is gone. The shell exposes `pagemap <addr>` (full walk with flags)
and `vmtest` (map → write → read → unmap).

### 3. ELF loading, stack mapping and ring 3
- `src/elf.rs` validates an ELF64 header and maps every `PT_LOAD` segment with
  the permissions the segment asks for (`PF_R/PF_W/PF_X` → present/writable/NX);
  `.bss` is zero because the frames are fresh.
- `src/usermode.rs` creates the address space, maps a 64 KiB user stack, lays
  out the initial `argc/argv/envp/auxv` frame, points `TSS.rsp0` at a kernel
  stack and `iretq`s to the ELF entry point in ring 3. When the program calls
  `exit` (or is killed by a fault) the kernel resumes on its own stack, records
  the status and frees every frame of the process.
- `src/user_program.rs` carries two complete ELF executables assembled into the
  kernel image (there is no disk driver yet): `hello` and `crash`.
- The shell runs them: `exec hello`, `exec crash`, `elf hello` (header dump).

### 4. System calls from user space
`src/syscall.rs` dispatches `int 0x80` (gate DPL 3) from the shared trap frame:

| nr | call | notes |
|----|------|-------|
| 0 | `read(fd, buf, len)` | keyboard line input, `/dev/zero` |
| 1 | `write(fd, buf, len)` | screen / serial / `/dev/null`, honours the fd |
| 2 | `open(path, flags, mode)` | devfs lookup, lowest free descriptor |
| 3 | `close(fd)` | |
| 12 | `brk(addr)` | grows/shrinks the heap by mapping real frames |
| 39 | `getpid()` | |
| 60 | `exit(status)` | returns to the kernel, tears the process down |

Pointers coming from ring 3 are never dereferenced directly: `src/uaccess.rs`
walks the process page tables and rejects unmapped, kernel-only or read-only
buffers (`copy_from_user`, `copy_to_user`, `copy_str_from_user`), so a bad
pointer gives `-EFAULT` instead of a kernel fault. A per-process descriptor
table lives in `src/fd.rs`, and faults in ring 3 kill the process (status 139)
instead of the machine.

### Interactive shell
`src/shell.rs` — prompt `ferrumix> ` on screen (green) and serial, line editing
with backspace. Commands: `help`, `clear`, `echo`, `console`, `ps`, `mem`,
`pagemap`, `vmtest`, `elf`, `exec`, `ls [/bin|/dev]`, `uptime`, `uname`,
`whoami`, `version`, `reboot`.

**Dependencies:** zero external crates — only `core` and stable Rust
(`asm!`, `global_asm!`).

## Build and run

Required: Rust (stable), the `x86_64-unknown-none` target, QEMU.

```bash
# 1. Install the target and (if needed) rust-lld
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview   # provides rust-lld

# 2. Build the kernel
cargo build --target x86_64-unknown-none      # or: make build

# 3. Build with the custom Unix target (nightly needed)
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo +nightly build --target x86_64-ferrumix.json -Zbuild-std=core,compiler_builtins

# 4. Run in QEMU
make run             # VGA window + serial in the terminal
make run-headless    # everything on the serial line
```

The kernel is linked non-PIE with the static relocation model (`.cargo/config.toml`):
a kernel lives at a fixed physical address and its boot trampoline and stub
tables use absolute addresses.

Expected output (serial):

```
Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust
console: text output ready (target: both, VGA 80x25 text mode + COM1 0x3f8)
boot protocol: pvh, usable RAM: 127 MiB in 3 region(s)
frame allocator: 2 regions, 32000 total frames (125 MiB), ...
paging: identity map of the first 1 GiB active (2 MiB pages)
paging: NX bit enabled (EFER.NXE) — user data pages are non-executable
paging: self-test OK — mapped 0x400000000 -> 0x..., wrote/read 0x..., unmapped
GDT + TSS initialised (kernel + ring-3 segments, IST, rsp0)
process table: pid 1 running, 1 entries used
VFS: devfs mounted at /dev, 5 devices
IDT + PIC + PIT initialised; interrupts enabled
syscall: int 0x80 gate ready (DPL 3) — read, write, open, close, brk, getpid, exit
syscall: int 0x80 self-test from ring 0 works
Ferrumix is alive.
elf: hello loaded — entry 0x8000001080, 1 segment(s), 1 page(s), brk 0x8000002000
usermode: pid 2 entering ring 3 at 0x8000001080, stack 0x80007fff80 (cr3 0x...)
hello from ring 3: userspace ELF calling the kernel via int 0x80
ring 3 -> /dev/ttyS0: this line was written to the serial port
usermode: pid 2 exited with status 7 (5 user frames freed)
init: ring 3 program finished with status 7
Type 'help' for a list of commands.
ferrumix>
```

## Continuous Integration

All builds and tests run in **GitHub Actions** — this is the authoritative
verification, and `make test` mirrors it locally:

```bash
make test            # fmt + clippy + build + QEMU boot test
make test-release    # same for the release profile
```

The boot test asserts the banner, the memory/paging/GDT/IDT/syscall/VFS
subsystems, the paging and syscall self-tests, the ELF load, the line printed
by the ring-3 program, its exit status and the shell prompt. A freestanding
kernel cannot use `cargo test`; QEMU is its test harness.

## Structure

```
ferrumix/
├── Cargo.toml            # kernel package, panic="abort", no dependencies
├── .cargo/config.toml    # target, static relocation model, non-PIE, linker script
├── x86_64-ferrumix.json  # custom Unix target (code-model=kernel, no redzone)
├── linker.ld             # kernel at 1 MiB, boot headers first, PVH note, bss bounds
├── Makefile              # build / run / test / fmt / clippy
├── src/
│   ├── main.rs           # kernel_main: bring-up, run an ELF in ring 3, shell
│   ├── boot.rs/.S        # boot headers (Multiboot2 + PVH) and long-mode trampoline
│   ├── console.rs        # text output: screen and/or serial, print!/println!
│   ├── vga.rs            # VGA text mode driver
│   ├── serial.rs         # COM1 driver + serial_println!
│   ├── port.rs           # port I/O, hlt/sti/cli/pushcli/popcli
│   ├── spinlock.rs       # Spinlock + IntSpinlock (interrupt-safe)
│   ├── gdt.rs            # GDT (kernel + ring 3) and TSS (IST, rsp0)
│   ├── idt.rs            # IDT entries and lidt
│   ├── interrupts.rs     # assembly ISR stubs, trap frame, PIC/PIT/keyboard
│   ├── multiboot.rs      # PVH / Multiboot1 / Multiboot2 memory maps
│   ├── memory.rs         # physical frame allocator (4 KiB, bump + free list)
│   ├── paging.rs         # map/unmap/split/translate, address spaces, NX, TLB
│   ├── uaccess.rs        # copy_from_user / copy_to_user with page-walk checks
│   ├── elf.rs            # ELF64 parser and loader
│   ├── usermode.rs       # user stack, iretq into ring 3, exit/fault return
│   ├── user_program.rs   # embedded ELF programs (hello, crash)
│   ├── fd.rs             # per-process file descriptor table
│   ├── syscall.rs        # int 0x80 dispatch (read/write/open/close/brk/getpid/exit)
│   ├── process.rs        # process table (pid, state, address space, exit status)
│   ├── kb_buffer.rs      # keyboard character ring buffer
│   ├── shell.rs          # interactive shell
│   └── vfs.rs            # devfs: null, zero, tty, console, ttyS0
├── userspace/            # freestanding user program scaffold (int 0x80 ABI)
└── docs/ROADMAP.md       # plan and checklist
```
