//! The interactive Ferrumix shell.
//!
//! Reads a line from the keyboard (with echo and backspace), splits it into a
//! command and arguments and runs one of the built-ins.  Besides the usual
//! `help`/`ps`/`mem` conveniences it exposes the new subsystems: `console`
//! switches the text output between screen and serial, `pagemap`/`vmtest`
//! poke at the paging code, and `exec` loads one of the embedded ELF programs
//! and runs it in ring 3.

use crate::console;
use crate::interrupts;
use crate::memory;
use crate::paging;
use crate::process;
use crate::user_program;
use crate::usermode;

const LINE_MAX: usize = 256;

/// Print the shell prompt (coloured on the screen, plain on the serial line).
fn print_prompt() {
    use core::fmt::Write;
    {
        let mut screen = crate::vga::WRITER.lock();
        screen.set_color(crate::vga::Color::LightGreen, crate::vga::Color::Black);
        let _ = screen.write_str("ferrumix");
        screen.set_color(crate::vga::Color::White, crate::vga::Color::Black);
        let _ = screen.write_str("> ");
    }
    {
        let mut uart = crate::serial::SERIAL.lock();
        let _ = uart.write_str("ferrumix> ");
    }
}

/// Read one line from the keyboard, echoing as we go.  Blocks until Enter.
fn read_line(buf: &mut [u8]) -> usize {
    let mut len: usize = 0;
    loop {
        match crate::kb_buffer::try_read_char() {
            Some(0x08) => {
                if len > 0 {
                    len -= 1;
                    let mut screen = crate::vga::WRITER.lock();
                    if screen.col > 0 {
                        screen.col -= 1;
                        screen.write_byte(b' ');
                        screen.col -= 1;
                    }
                }
            }
            Some(b'\n') => {
                if len < buf.len() {
                    buf[len] = b'\n';
                    len += 1;
                }
                crate::println!();
                return len;
            }
            Some(byte) => {
                if len < buf.len() - 1 {
                    buf[len] = byte;
                    len += 1;
                    console::write_bytes_to(console::Target::Both, &[byte]);
                }
            }
            None => unsafe {
                // Nothing buffered: sleep until the next interrupt.
                core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
            },
        }
    }
}

/// Run the shell loop.  Never returns.
pub fn run() -> ! {
    crate::println!("Type 'help' for a list of commands.");
    print_prompt();

    let mut line_buf = [0u8; LINE_MAX];
    loop {
        let len = read_line(&mut line_buf);
        let line_len = if len > 0 && line_buf[len - 1] == b'\n' {
            len - 1
        } else {
            len
        };
        let line = core::str::from_utf8(&line_buf[..line_len]).unwrap_or("");
        let line = line.trim();
        if !line.is_empty() {
            dispatch(line);
        }
        print_prompt();
    }
}

fn dispatch(line: &str) {
    let (cmd, args) = match line.find(' ') {
        Some(pos) => (&line[..pos], line[pos + 1..].trim()),
        None => (line, ""),
    };

    match cmd {
        "help" => cmd_help(),
        "clear" => cmd_clear(),
        "echo" => cmd_echo(args),
        "ps" => cmd_ps(),
        "uptime" => cmd_uptime(),
        "mem" | "memory" => cmd_mem(),
        "whoami" => cmd_whoami(),
        "uname" => cmd_uname(),
        "reboot" => cmd_reboot(),
        "ls" => cmd_ls(args),
        "ver" | "version" => cmd_version(),
        "console" => cmd_console(args),
        "pagemap" => cmd_pagemap(args),
        "vmtest" => cmd_vmtest(),
        "exec" | "run" => cmd_exec(args),
        "elf" => cmd_elf(args),
        _ => crate::println!("{}: command not found (type 'help')", cmd),
    }
}

fn cmd_help() {
    crate::println!("Ferrumix shell — built-in commands:");
    crate::println!("  help                 this text");
    crate::println!("  clear                clear the screen");
    crate::println!("  echo <text>          print text");
    crate::println!("  console <target>     text output: screen | serial | both");
    crate::println!("  ps                   list processes");
    crate::println!("  mem                  physical memory statistics");
    crate::println!("  pagemap <addr>       walk the page tables for an address");
    crate::println!("  vmtest               map / write / read / unmap a test page");
    crate::println!("  elf [name]           inspect an embedded ELF program");
    crate::println!("  exec <name>          run a program in ring 3 (hello, crash)");
    crate::println!("  ls [/bin|/dev]       list devices or programs");
    crate::println!("  uptime               time since boot");
    crate::println!("  uname                system information");
    crate::println!("  whoami               current user");
    crate::println!("  version              kernel version");
    crate::println!("  reboot               reset the machine");
}

fn cmd_clear() {
    crate::vga::WRITER.lock().clear();
}

fn cmd_echo(args: &str) {
    crate::println!("{}", args);
}

fn cmd_console(args: &str) {
    if args.is_empty() {
        let name = console::target().as_str();
        crate::println!("console: current target is '{}'", name);
        crate::println!("usage: console screen|serial|both");
        return;
    }
    match console::Target::parse(args) {
        Some(target) => {
            console::set_target(target);
            // Announce on both sinks so the change is visible either way.
            console::write_bytes_to(console::Target::Both, b"console: target is now ");
            console::write_bytes_to(console::Target::Both, target.as_str().as_bytes());
            console::write_bytes_to(console::Target::Both, b"\n");
        }
        None => crate::println!("console: unknown target '{}'", args),
    }
}

fn cmd_ps() {
    let mut buf = [process::Process::empty(); 16];
    let count = process::snapshot(&mut buf);
    crate::println!("  PID  PPID STATE     CR3          NAME");
    for entry in buf.iter().take(count) {
        crate::println!(
            "  {:<4} {:<4} {:<9} {:#012x} {}",
            entry.pid,
            entry.ppid,
            entry.state.as_str(),
            entry.root,
            entry.name()
        );
    }
}

fn cmd_uptime() {
    let ticks = interrupts::get_ticks();
    crate::println!(
        "up {}.{:02} s ({} timer ticks at 100 Hz)",
        ticks / 100,
        ticks % 100,
        ticks
    );
}

fn cmd_mem() {
    let (total, used, free) = memory::stats();
    crate::println!(
        "frames: {} total ({} MiB), {} used, {} free",
        total,
        total * memory::FRAME_SIZE / (1024 * 1024),
        used,
        free
    );
    crate::println!(
        "paging: CR3 {:#x}, NX {}",
        paging::active_root(),
        if paging::nx_enabled() {
            "enabled"
        } else {
            "unavailable"
        }
    );
}

fn parse_addr(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).ok()
}

fn cmd_pagemap(args: &str) {
    let addr = match parse_addr(args) {
        Some(addr) => addr,
        None => {
            crate::println!("usage: pagemap <hex address>   (e.g. pagemap b8000)");
            return;
        }
    };
    let root = paging::active_root();
    crate::println!("page table walk for {:#x} in CR3 {:#x}:", addr, root);
    paging::dump_walk(root, addr);
}

fn cmd_vmtest() {
    let virt = paging::KERNEL_SCRATCH_BASE + paging::PAGE_SIZE;
    let root = paging::active_root();
    let frame = match memory::alloc_frame() {
        Some(frame) => frame,
        None => {
            crate::println!("vmtest: out of physical memory");
            return;
        }
    };
    let flags = paging::ENTRY_PRESENT | paging::ENTRY_WRITABLE;
    match unsafe { paging::map_page(root, virt, frame, flags) } {
        Ok(()) => crate::println!("vmtest: mapped {:#x} -> frame {:#x}", virt, frame),
        Err(err) => {
            crate::println!("vmtest: map failed: {:?}", err);
            memory::free_frame(frame);
            return;
        }
    }

    let magic: u64 = 0xdead_beef_cafe_f00d;
    unsafe { core::ptr::write_volatile(virt as *mut u64, magic) };
    let via_virt = unsafe { core::ptr::read_volatile(virt as *const u64) };
    let via_phys = unsafe { core::ptr::read_volatile(frame as *const u64) };
    crate::println!(
        "vmtest: wrote {:#x}, read {:#x} via virtual, {:#x} via physical",
        magic,
        via_virt,
        via_phys
    );

    match unsafe { paging::unmap_page(root, virt) } {
        Ok(freed) => crate::println!("vmtest: unmapped {:#x} (frame {:#x})", virt, freed),
        Err(err) => crate::println!("vmtest: unmap failed: {:?}", err),
    }
    crate::println!(
        "vmtest: translation after unmap: {:?}",
        paging::translate(virt)
    );
    memory::free_frame(frame);
}

fn cmd_elf(args: &str) {
    let name = if args.is_empty() { "hello" } else { args };
    let (name, image) = match user_program::find(name) {
        Some(found) => found,
        None => {
            crate::println!("elf: no such program '{}'", name);
            return;
        }
    };
    match crate::elf::inspect(image) {
        Ok(info) => crate::println!(
            "elf: {} — {} bytes, entry {:#x}, {} program header(s), {} loadable",
            name,
            info.size,
            info.entry,
            info.phnum,
            info.loadable
        ),
        Err(err) => crate::println!("elf: {} is not loadable: {:?}", name, err),
    }
}

fn cmd_exec(args: &str) {
    if args.is_empty() {
        crate::println!("usage: exec <name>   (available: hello, crash)");
        return;
    }
    let (name, image) = match user_program::find(args) {
        Some(found) => found,
        None => {
            crate::println!("exec: no such program '{}'", args);
            return;
        }
    };
    match usermode::exec(name, image) {
        Ok(status) => crate::println!("exec: {} finished with status {}", name, status),
        Err(err) => crate::println!("exec: {} failed: {:?}", name, err),
    }
}

fn cmd_whoami() {
    crate::println!("root");
}

fn cmd_uname() {
    crate::println!("Ferrumix 0.1.0 x86_64 (Multiboot2, long mode, ring 3 capable)");
}

fn cmd_version() {
    crate::println!("Ferrumix kernel 0.1.0 — a tiny Unix-like kernel in Rust");
}

fn cmd_ls(args: &str) {
    match args {
        "/bin" | "bin" => {
            for name in user_program::NAMES.iter() {
                crate::println!("{}", name);
            }
        }
        _ => crate::vfs::list(),
    }
}

fn cmd_reboot() {
    crate::println!("rebooting...");
    unsafe {
        // Pulse the 8042 reset line.
        crate::port::outb(0x64, 0xfe);
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}
