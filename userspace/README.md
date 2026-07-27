# Ferrumix userspace

This directory is the **scaffold** for Ferrumix's ring-3 user programs. It is
not built or loaded by the kernel yet — the kernel currently runs only in
ring 0. The goal is for the kernel to eventually:

1. Set up proper page tables with a higher-half kernel mapping and per-process
   user address spaces.
2. Switch to ring 3 via an `iret`/`sysret` into a loaded ELF (this `_start`).
3. Service `int 0x80` (later `syscall`/`sysenter`) traps in the kernel's
   interrupt/`syscall` handler.

## System-call ABI (draft)

| nr | name  | args                         | returns |
|----|-------|------------------------------|---------|
| 0  | exit  | `code: usize`                | `!`     |
| 1  | write | `fd`, `buf*`, `len`          | `usize` |

Registers: `rax` = syscall number, `rdi`/`rsi`/`rdx` = arguments.

## Building (later)

A dedicated target (e.g. `x86_64-ferrumix`) plus a freestanding
`bare-metal` linker script will link these programs as flat ELF binaries that
the kernel's loader maps into a new process address space. Until then, treat
this folder as documentation of intent.
