# Ferrumix userspace

Ring-3 programs for Ferrumix. The kernel side is complete: it creates a
private address space, loads an ELF64 image, maps a stack, drops to ring 3 and
services `int 0x80` system calls from it (see `src/usermode.rs`,
`src/elf.rs`, `src/syscall.rs`).

There is no disk driver yet, so the two demo programs the kernel can run
(`exec hello`, `exec crash`) are assembled into the kernel image itself as
complete ELF executables — see `src/user_program.rs`. This directory is the
scaffold for programs built as ordinary Rust binaries; once a filesystem
exists they will be loaded from `/bin` with the very same loader.

## System-call ABI (implemented)

| nr | name   | args                         | returns | notes |
|----|--------|------------------------------|---------|-------|
| 0  | read   | `fd`, `buf*`, `len`          | bytes   | fd 0 blocks on the keyboard, `/dev/zero` gives zeroes |
| 1  | write  | `fd`, `buf*`, `len`          | bytes   | fd 1/2 → console, `/dev/tty` → screen, `/dev/ttyS0` → serial |
| 2  | open   | `path*`, `flags`, `mode`     | `fd`    | devfs: `/dev/tty`, `/dev/ttyS0`, `/dev/console`, `/dev/null`, `/dev/zero` |
| 3  | close  | `fd`                         | `0`     | |
| 12 | brk    | `addr`                       | break   | 0 queries, otherwise maps/unmaps real frames |
| 39 | getpid | —                            | `pid`   | from the process table |
| 60 | exit   | `status`                     | `!`     | returns control to the kernel |

Registers: `rax` = call number, `rdi`/`rsi`/`rdx` = arguments, `rax` = result
or a negative errno (`-EFAULT`, `-EBADF`, `-ENOENT`, `-ENOSYS`, ...). All
other registers are preserved by the kernel's trap stub.

Every pointer is validated by the kernel with a page-table walk
(`copy_from_user` / `copy_to_user`), so passing a bad pointer returns
`-EFAULT`; a genuine fault in ring 3 (like the `crash` demo) kills the process
with status 139 and returns to the shell.

## Memory layout of a process

```text
  0x00_0000_1000            first valid user address (page 0 stays unmapped)
  0x80_0000_1000            ELF image (PT_LOAD segments, r-x / rw- as requested)
  ...                       brk heap grows up from the end of the image
  0x80_007f_0000            user stack (64 KiB)
  0x80_0080_0000            top of stack: argc / argv / envp / auxv frame
```

## Building

```bash
# stable — freestanding binary for the bare-metal target
cargo build --target x86_64-unknown-none --manifest-path userspace/Cargo.toml
# the kernel's custom target (nightly + build-std)
cargo +nightly build --target x86_64-ferrumix.json \
    -Zbuild-std=core,compiler_builtins --manifest-path userspace/Cargo.toml
```

To be loadable the binary has to be a static `ET_EXEC` ELF64 linked at
`0x80_0000_1000` (see `usermode::USER_TEXT_BASE`).

## Example

```rust
#![no_std]
#![no_main]
mod syscall;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    syscall::write(1, b"hello from ferrumix userspace\n");
    let fd = syscall::open("/dev/ttyS0\0");
    syscall::write(fd, b"...and this goes to the serial port only\n");
    syscall::close(fd);
    syscall::exit(0);
}
```
