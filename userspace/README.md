# Ferrumix userspace

This directory is the scaffold for Ferrumix's ring-3 user programs. After the
first Unix step the kernel already has:

- GDT with user code/data (DPL3) and TSS.rsp0
- IDT gate for `int 0x80` at DPL3
- frame allocator and process table
- syscall handler (`write`, `exit`, `getpid`)

So the ABI below is now **functional** from ring 0 self-tests (`kernel_main`
does `int 0x80` write). Ring-3 entry via `iret`/`sysret` into ELF is next,
but the gate is already usable and tested in CI.

## System-call ABI (implemented)

| nr | name   | args                         | returns | notes |
|----|--------|------------------------------|---------|-------|
| 0  | exit   | `code: usize`                | `!`     | also 60 (Linux compat) |
| 1  | write  | `fd`, `buf*`, `len`          | `usize` | stdout=1, stderr=2 → VGA+serial |
| 12 | brk    | `addr`                       | `usize` | stub, returns 0 |
| 39 | getpid | —                            | `pid`   | from process table |

Registers: `rax` = syscall number, `rdi`/`rsi`/`rdx` = arguments, `rax` = return
(or negative errno). `rcx`/`r11` are clobbered (as in `syscall` instruction
convention) — caller-save.

## Building

```bash
# stable — uses x86_64-unknown-none (works, freestanding)
cargo build --target x86_64-unknown-none --manifest-path userspace/Cargo.toml
# custom Unix target — nightly + build-std (matches kernel's custom target)
cargo +nightly build --target x86_64-ferrumix.json -Zbuild-std=core,compiler_builtins --manifest-path userspace/Cargo.toml
```

Eventually the kernel's ELF loader will map these binaries into a new process
address space and `iret` into `_start`. Until then this folder serves as the
reference implementation of the ABI and is built in CI (`build-userspace` job).

## Example _start

```rust
#![no_std]
#![no_main]
mod syscall;
#[no_mangle]
pub extern "C" fn _start() -> ! {
    syscall::write(1, b"hello from ferrumix userspace\n");
    syscall::exit(0);
}
```
