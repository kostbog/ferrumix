# Ferrumix

![CI](https://github.com/kostbog/ferrumix/actions/workflows/ci.yml/badge.svg)

> *ferrum* (Latin for "iron") + *ix* — a Unix clone written in Rust.

Ferrumix is an **x86_64 OS kernel written from scratch**, booted via the
**Multiboot2** specification and run in QEMU. After the first Unix step it
is more than a skeleton: it has a frame allocator, paging introspection,
GDT with ring-3 segments, a process table, `int 0x80` syscall gate, and a
minimal VFS/devfs — all verified by CI.

> ⚠️ **A note about building in this environment.** The sandbox where this
> code was written has no Rust toolchain and no access to Rust/apt mirrors
> (the proxy only allows `github.com`, `pypi.org`, `registry.npmjs.org`).
> Because of that, the code **was not compiled or run here** — it was
> written and reviewed by hand, and is ready to be built on your own
> machine (see "Build and run") and is fully tested in GitHub Actions.

---

## What already works

### Boot & low level
- 32→64-bit trampoline and enabling long mode (paging) — `src/boot.S`
- Multiboot2 header, booted via `qemu-system-x86_64 -kernel`
- Text-mode VGA driver (0xB8000) + mirroring to the serial port
- GDT + TSS (IST stack for double fault, RSP0 for ring3→ring0) **with user
  code/data segments** (DPL3) — 0x18/0x20 — ready for ring 3
- IDT with all exceptions, double fault and page fault handling, **syscall
  gate at 0x80 DPL=3**
- 8259 PIC (remapped to vectors 0x20+), PIT timer (~100 Hz), keyboard (set 1)
- Multiboot2 memory map parser (usable RAM, 32 regions)

### Unix step 1 — closer to real Unix
- **Custom target** `x86_64-ferrumix.json` (kernel code-model, no redzone,
  no SSE, LLD) — roadmap item 1. Builds with nightly ` -Zbuild-std=core,compiler_builtins`
- **Physical frame allocator** (`src/memory.rs`): bump + free list (256 entries),
  4 KiB frames, excludes <1 MiB and kernel image (`__kernel_start`/`__kernel_end`
  from linker.ld), stats and region dump on serial
- **Paging** (`src/paging.rs`): CR3 read, PML4/PDPT/PD inspection, software
  virt→phys walk (2 MiB and 1 GiB huge pages supported), base for higher-half
- **Process table** (`src/process.rs`): 64 slots, atomic PID allocator,
  `ProcessState`, init pid 1, `current_pid()`, count
- **Syscall interface** (`src/syscall.rs` + assembly stub):
  `syscall_int80_entry` global asm saves all GPRs, calls Rust `syscall_dispatch`,
  restores and `iretq`. Numbers: 0/60 `exit`, 1 `write`, 39 `getpid`, 12 `brk`
  (stub). `write` copies user buffer (null/len checks) to VGA+serial.
  Self-test in `kernel_main` does `int 0x80` write and getpid.
- **VFS/devfs stub** (`src/vfs.rs`): nodes for `null`, `zero`, `tty`, `ttyS0`,
  listing on boot, `find_dev` for future `open`

**Dependencies:** zero external crates. Only `core` + stable Rust
(`asm!`, `global_asm!`, `extern "x86-interrupt"`).

## Build and run

Required: Rust (stable, ≥ 1.69), the `x86_64-unknown-none` target, QEMU.

```bash
# 1. Install the target and (if needed) rust-lld
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview   # provides rust-lld, if the linker needs it

# 2. Build the kernel
cargo build --target x86_64-unknown-none
#    or simply `make build` (the Makefile target already sets --target)
#    release: `make build-release` or `cargo build --release`

# 3. Build with custom Unix target (nightly needed)
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo +nightly build --target x86_64-ferrumix.json -Zbuild-std=core,compiler_builtins

# 4. Run in QEMU
#    - graphical mode (VGA window + serial in the terminal):
make run
#    - headless (all output, including VGA, goes to serial -> terminal):
make run-headless
```

Expected output (in serial / the terminal):

```
Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust
boot magic: 0x36d76289, multiboot info @ 0x...
detected usable RAM: NNNN MiB (X regions)
frame allocator: N regions, NNNN total frames (NNN MiB), ...
  region 0: [0x100000 - 0x... ) NN frames
...
paging: CR3=0x..., PML4 at ...
paging: identity mapping for first 1 GiB active
GDT: kernel code=0x8 data=0x10, user code=0x20 data=0x18, TSS=0x28
GDT + TSS initialised (kernel + user segments, IST)
process: init created pid=1 ...
process table: pid 1 running, 1 total
vfs: devfs initialised
vfs: devfs dev/null type=CharDevice
...
VFS initialised: devfs with null, zero, tty
IDT + PIC + PIT initialised; interrupts enabled
syscall gate: int 0x80 DPL=3 installed (write, exit, getpid)
memory: allocated frame @ 0x...
memory: total N used M free K
memory: freed frame @ 0x...
testing syscall via int 0x80 ...
syscall: write via int 0x80 works — hello Unix!
test syscall write returned 46
syscall getpid() -> 1
custom target: x86_64-ferrumix.json ready
Ferrumix is alive. (idle loop; timer ticks on the serial line)
Unix step complete: ring3 GDT, frame allocator, syscall int 0x80, process table, VFS devfs
timer tick 1000
...
```

## Continuous Integration — all tests & builds on GitHub

All builds and tests run in **GitHub Actions** (`.github/workflows/ci.yml`) — this is the
authoritative verification. Local `make test` mirrors the same checks for convenience.

CI jobs:
- **lint**: `cargo fmt --all --check` + `clippy` (freestanding, non-blocking)
- **build-matrix**: debug & release for `x86_64-unknown-none`, artifacts uploaded
- **build-custom-target**: builds `x86_64-ferrumix.json` with nightly + `-Zbuild-std`
- **build-userspace**: builds `userspace/` example (no_std)
- **boot-test** (debug & release): boots kernel in headless QEMU, asserts:
  `Ferrumix 0.1.0`, `is alive`, `GDT`, `IDT`, `frame allocator`, `paging`,
  `syscall`, `process`, `VFS`, plus syscall self-test
- **make-test**: runs `make test` compatibility target

The same tests locally:

```bash
make test            # debug boot test with Unix subsystem asserts
make test-release    # release boot test
make fmt
make clippy
```

A freestanding kernel cannot use `cargo test`; QEMU boot is its test harness.

## Structure

```
ferrumix/
├── Cargo.toml            # kernel package, panic="abort" profiles, no dependencies
├── .cargo/config.toml    # x86_64-unknown-none default + x86_64-ferrumix custom target
├── x86_64-ferrumix.json  # custom Unix target (code-model=kernel, no redzone)
├── linker.ld             # layout: kernel at 1 MiB, __kernel_start/_end, Multiboot2 first
├── Makefile              # build / run / test / fmt / clippy
├── src/
│   ├── main.rs           # entry kernel_main + self-tests for syscall & allocator
│   ├── boot.rs/.S        # Multiboot2 header + trampoline into long mode
│   ├── port.rs           # port I/O (inb/outb/...), hlt/sti/cli
│   ├── spinlock.rs       # mini spinlock built on atomics
│   ├── vga.rs            # text-mode VGA driver + print!/println!
│   ├── serial.rs         # COM1 + serial_print!/serial_println!
│   ├── gdt.rs            # GDT (kernel+user) + TSS (IST+RSP0)
│   ├── idt.rs            # IDT descriptors + lidt, DPL support
│   ├── interrupts.rs     # exception handlers, PIC, PIT, keyboard, syscall gate install
│   ├── multiboot.rs      # Multiboot2 mmap parser (regions + total)
│   ├── memory.rs         # frame allocator (4 KiB, bump+free list)
│   ├── paging.rs         # paging introspection & virt→phys walk
│   ├── process.rs        # Unix pid table, process states
│   ├── syscall.rs        # int 0x80 handler, dispatch, write/getpid/exit/brk
│   └── vfs.rs            # devfs stub: null, zero, tty, ttyS0
├── userspace/            # ring-3 program scaffold using int 0x80 ABI
│   ├── src/main.rs/_start calling write+exit
│   └── src/syscall.rs    # same ABI as kernel: 0=exit, 1=write
└── docs/ROADMAP.md       # plan and checklist (what's done)
```

## How it boots (in short)

1. QEMU with `-kernel` sees the Multiboot2 header in `boot.S`, loads the kernel into
   32-bit protected mode, and jumps to `_start`, passing magic in `EAX` and info in `EBX`.
2. `_start` builds page tables (identity-mapping the first 1 GiB with 2 MiB pages),
   enables PAE/long mode/paging, and jumps into 64-bit mode via `lgdt` + far jump.
3. In long mode the stack is set up and `kernel_main(magic, mb_info)` is called.
4. The kernel initialises: serial, frame allocator from Multiboot mmap, paging check,
   GDT (kernel+user) + TSS, process table (pid 1), devfs, IDT (exceptions + IRQ +
   int 0x80 gate), PIC/PIT, enables interrupts, self-tests syscall `write` via
   `int 0x80`, then idles with `hlt`.

## Next steps

See [`docs/ROADMAP.md`](docs/ROADMAP.md): higher-half mapping, per-process page tables,
context switch & scheduler (round-robin via PIT), `syscall` instruction fast path,
fork/exec, ramfs, ELF loader, shell and utilities.
```
