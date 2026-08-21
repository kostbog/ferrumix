# Contributing to Ferrumix

Thanks for wanting to help. Ferrumix is a from-scratch **x86_64 Unix-like kernel**
in Rust: Multiboot2 boot, no `std`, **zero external crates**. Small, focused
patches are easier to review than large rewrites.

Please read [README.md](README.md) for what already works and
[docs/ROADMAP.md](docs/ROADMAP.md) before starting non-trivial work.

## Ways to help

- Fix bugs, panics, or boot-test flakes.
- Implement a **checked-off-next** item from the roadmap (one step at a time).
- Improve comments, module docs, or the boot/serial diagnostics.
- Add a shell builtin, a syscall, or a userspace ABI helper — if it fits the
  current architecture (see below).
- Tighten CI or Makefile targets without changing the kernel contract.

If you are unsure whether an idea belongs, open an issue first.

## Development setup

You need:

- Rust **stable ≥ 1.69** (`rustup`)
- Target `x86_64-unknown-none`
- `llvm-tools-preview` (provides `rust-lld` when the linker needs it)
- QEMU (`qemu-system-x86_64`)
- `rustfmt` and `clippy`
- Optional: **nightly** + `rust-src` for the custom target `x86_64-ferrumix.json`

```bash
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview rustfmt clippy
# optional custom Unix target
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
```

On Debian/Ubuntu, QEMU is `qemu-system-x86`.

## Build, run, test

The Makefile is the local entry point; GitHub Actions
(`.github/workflows/test.yml`) is the source of truth.

```bash
make build              # debug kernel, x86_64-unknown-none
make build-release
make run                # QEMU window + serial on stdio
make run-headless       # serial only (what CI uses)
make fmt                # cargo fmt --all --check
make clippy
make test               # fmt + clippy + debug build + QEMU boot asserts
make test-release
```

Custom target (nightly):

```bash
make build-custom
# or
cargo +nightly build --target x86_64-ferrumix.json -Zbuild-std=core,compiler_builtins
```

Userspace scaffold:

```bash
cargo build --target x86_64-unknown-none --manifest-path userspace/Cargo.toml
```

A freestanding kernel **cannot** use `cargo test`. The harness is a headless
QEMU boot that greps serial output. If you change boot messages, keep the
strings that `make test` and the `boot-test` job look for, or update **both**
the Makefile and the workflow.

Currently asserted (debug):

- `Ferrumix 0.1.0`
- `is alive`
- `GDT`, `IDT`, `frame allocator`, `paging`, `syscall`, `process`, `VFS`
- shell prompt `ferrumix>`

## Hard rules for kernel code

These are architectural, not style nits:

1. **No external crates.** Kernel and userspace stay on `core` only.
2. **`#![no_std]` / `#![no_main]`.** No libc, no unwinding.
3. **`panic = "abort"`** in both profiles. Do not add a personality /
   `_Unwind_Resume`.
4. **Do not enable the red zone, SSE, or a userspace code model** in kernel
   code. The custom target already sets `disable-redzone`, `-sse`, and
   `code-model: kernel`.
5. Keep the **Multiboot2 header first** and the kernel loaded at **1 MiB**
   (`linker.ld`). Changing layout is a boot-breaking change — document it
   and extend the boot test.
6. Default build target remains **`x86_64-unknown-none`** (stable). The
   JSON target is extra coverage, not a replacement.

## Code style

- `rustfmt` on everything that `cargo fmt` can touch. CI fails on
  `cargo fmt --all --check`.
- Module-level `//!` docs on each `src/*.rs` file: what it is, how it is
  entered, and any ABI or hardware contract.
- Prefer small functions, explicit `unsafe` blocks, and a one-line comment
  on **why** the block is sound (not what the instructions do).
- British/American spelling is not policed; **be consistent with nearby
  comments**.
- Syscall numbers follow the **Linux x86_64** table where we implement the
  same name (`read` = 0, `write` = 1, `exit` = 60, …). New calls should
  reuse those numbers, not invent a private ABI.
- Registers for `int 0x80`: `rax` = number, `rdi`/`rsi`/`rdx` = args,
  `rax` = return (negative errno). Document any new argument in
  `userspace/README.md`.
- Interrupt handlers and the shell share VGA/serial: use `IntSpinlock`
  (or disable interrupts) so IRQ code cannot deadlock against `WRITER`.
- Do not busy-spin in the shell idle path; `hlt` until the next interrupt
  as `shell.rs` already does.

Assembly (`src/boot.S`, `global_asm!` stubs) should stay minimal and
commented. Match existing AT&T/GNU syntax.

## Where to put work

| Kind of change | Typical files |
| --- | --- |
| Boot / long mode | `src/boot.S`, `src/boot.rs`, `linker.ld` |
| CPU tables, IRQs, timer, keyboard IRQ | `src/gdt.rs`, `src/idt.rs`, `src/interrupts.rs` |
| Frame allocator / paging | `src/memory.rs`, `src/paging.rs`, `src/multiboot.rs` |
| Processes | `src/process.rs` |
| Syscalls | `src/syscall.rs` + `userspace/src/syscall.rs` |
| VFS / devices | `src/vfs.rs` |
| Shell commands | `src/shell.rs` |
| Keyboard buffer | `src/kb_buffer.rs` |
| Userspace example | `userspace/` |

Keep ring-3 programs out of the kernel crate until the ELF loader exists.
The in-kernel shell is a stopgap; do not grow it into a full userland.

## Adding a shell command

1. Parse it in `shell::dispatch`.
2. Implement `cmd_*` next to the others.
3. List it in `cmd_help` (and README if it is user-facing).
4. Prefer reading kernel state through existing modules (`process`,
   `memory`, `vfs`, `interrupts::get_ticks`) rather than reaching into
   statics from unrelated files.

## Adding a syscall

1. Assign a Linux-compatible number in `src/syscall.rs`.
2. Handle it in `syscall_dispatch`.
3. Mirror the wrapper in `userspace/src/syscall.rs`.
4. Update the ABI table in `userspace/README.md`.
5. If the call must be visible on serial during boot, keep messages
   stable enough that boot tests still pass.

Stubs (`open`/`close`/`brk` today) should return a documented fake value,
not panic.

## Pull requests

1. Branch from `main`. Keep the PR focused on one roadmap item or bug.
2. Run `make test` locally if you have QEMU; otherwise rely on CI, but
   do not skip fmt.
3. Describe **what** changed and **how you verified it** (boot log
   snippet, `make test`, QEMU).
4. Update README / ROADMAP checkboxes when you complete a listed item.
5. Do not vendor toolchains, `target/`, or disk images.

CI jobs that must stay green:

- `make test`
- debug/release kernel build (`x86_64-unknown-none`)
- custom target build (nightly + `-Zbuild-std`)
- userspace build
- QEMU `boot-test` (debug and release)

Clippy on the freestanding target is run with `-D warnings` in the
Makefile but is allowed to be non-blocking (`|| true`). Still fix
warnings you introduce.

## Commit messages

Short imperative subject, optional body. Examples:

- `Add ramfs nodes for / and /dev`
- `Fix keyboard set-1 release-code filter`
- `Document int 0x80 register ABI`

## Safety and scope

This is real kernel code: a bad page table or IDT change can triple-fault
in QEMU and look like a hang. Prefer identity-mapping and existing GDT
selectors unless your PR *is* the higher-half / ring-3 transition.

Do not submit exploits, host-side malware, or patches whose purpose is to
attack other systems. Fixing Ferrumix’s own robustness (page faults,
`copy_from_user`, bounds on syscall buffers) is welcome.

## License

The crate is dual-licensed **MIT OR Apache-2.0** (`Cargo.toml`).
Contributions are accepted under the same terms. There is no CLA.

## Questions

Use GitHub issues for design discussion. Point at the relevant ROADMAP
checkbox so the thread stays attached to a concrete step.
