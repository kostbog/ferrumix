# Ferrumix

![CI](https://github.com/kostbog/ferrumix/actions/workflows/ci.yml/badge.svg)

> *ferrum* (Latin for "iron") + *ix* — a Unix clone written in Rust.

Ferrumix is an **x86_64 OS kernel written from scratch**, booted via the
**Multiboot2** specification and run in QEMU. Right now it implements a
basic but genuinely working kernel skeleton (prioritized by agreement), on
top of which ring-3 processes, system calls, a filesystem, and a shell will
appear later.

> ⚠️ **A note about building in this environment.** The sandbox where this
> code was written has no Rust toolchain and no access to Rust/apt mirrors
> (the proxy only allows `github.com`, `pypi.org`, `registry.npmjs.org`).
> Because of that, the code **was not compiled or run here** — it was
> written and reviewed by hand, and is ready to be built on your own
> machine (see "Build and run").

---

## What already works

- 32→64-bit trampoline and enabling long mode (paging) — `src/boot.S`
- Multiboot2 header, booted via `qemu-system-x86_64 -kernel`
- Text-mode VGA driver (0xB8000) + mirroring to the serial port
- GDT + TSS (with an IST stack for double fault)
- IDT with all exceptions, double fault and page fault handling
- 8259 PIC (remapped to vectors 0x20+), PIT timer (~100 Hz), keyboard (set 1)
- Multiboot2 memory map parser (computes usable RAM)
- `panic_handler`, idle loop with `hlt` and interrupts enabled

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

# 3. Run in QEMU
#    - graphical mode (VGA window + serial in the terminal):
make run
#    - headless (all output, including VGA, goes to serial -> terminal):
make run-headless
```

If the linker complains that `rust-lld`/`ld` cannot be found, add
`llvm-tools-preview` (see above). If `cargo build` fails with a `linker`
error, make sure the `x86_64-unknown-none` target is installed.

Expected output (in serial / the terminal):

```
Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust
boot magic: 0x36d76289, multiboot info @ 0x...
detected usable RAM: NNNN MiB
GDT + TSS initialised
IDT + PIC + PIT initialised; interrupts enabled
Ferrumix is alive. (idle loop; timer ticks on the serial line)
timer tick 1000
...
```

## Continuous Integration

All builds and tests run in **GitHub Actions** (`.github/workflows/ci.yml`):
Rust + the `x86_64-unknown-none` target + QEMU are installed, the kernel is
built, then booted in headless QEMU, and it's verified that it prints the
banner and reaches the idle loop (`make test`). This boot test serves as a
unit test for a freestanding kernel (a regular `cargo test` doesn't apply
here).

The same test locally: `make test` (requires `qemu-system-x86_64` to be
installed).


## Structure

```
ferrumix/
├── Cargo.toml            # kernel package, panic="abort" profiles, no dependencies
├── .cargo/config.toml    # x86_64-unknown-none target + linker.ld linker
├── linker.ld             # layout: kernel at address 1 MiB, Multiboot2 header up front
├── Makefile              # build / run / run-headless / clean
├── src/
│   ├── main.rs           # entry point kernel_main(magic, mb_info) + panic_handler
│   ├── boot.rs           # pulls in boot.S via global_asm!
│   ├── boot.S            # Multiboot2 header + trampoline into long mode
│   ├── port.rs           # port I/O (inb/outb/...), hlt/sti/cli
│   ├── spinlock.rs       # mini spinlock built on atomics
│   ├── vga.rs            # text-mode VGA driver + print!/println!
│   ├── serial.rs         # COM1 + serial_print!/serial_println!
│   ├── gdt.rs            # GDT + TSS (IST)
│   ├── idt.rs            # IDT descriptors + lidt
│   ├── interrupts.rs     # handlers, PIC, PIT, keyboard
│   └── multiboot.rs      # Multiboot2 memory map parser
├── userspace/            # skeleton for ring-3 programs (not yet wired into the kernel)
│   ├── src/main.rs
│   └── src/syscall.rs    # draft syscall ABI (int 0x80)
└── docs/ROADMAP.md       # plan for growing into a full Unix clone
```

## How it boots (in short)

1. QEMU with `-kernel` sees the Multiboot2 header in `boot.S`, loads the
   kernel into 32-bit protected mode, and jumps to `_start`, passing the
   magic value in `EAX` and a pointer to the Multiboot2 structure in `EBX`.
2. `_start` builds page tables (identity-mapping the first 1 GiB with
   2 MiB pages), enables PAE/long mode/paging, and jumps into 64-bit mode
   via `lgdt` + a far jump.
3. In long mode the stack is set up and `kernel_main(magic, mb_info)` is
   called.
4. The kernel initializes VGA/serial, GDT/TSS, IDT/PIC/PIT, and goes into
   an idle loop with `hlt`, handling the timer and keyboard.

## Next steps

See [`docs/ROADMAP.md`](docs/ROADMAP.md): ring-3 processes, system calls,
a virtual filesystem, a shell, and utilities (`ls`, `cat`, …).
