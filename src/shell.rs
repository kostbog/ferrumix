//! Simple interactive shell for Ferrumix.
//!
//! Prints a prompt, reads characters from the keyboard buffer with echoing
//! and backspace support, assembles a line, and dispatches built-in commands.
//! This is the primary user-facing interface of the OS.

use crate::process;

const LINE_MAX: usize = 256;

/// Print the shell prompt to VGA (colored) and serial.
fn print_prompt() {
    use core::fmt::Write;
    {
        let mut w = crate::vga::WRITER.lock();
        w.set_color(crate::vga::Color::LightGreen, crate::vga::Color::Black);
        let _ = w.write_str("ferrumix");
        w.set_color(crate::vga::Color::White, crate::vga::Color::Black);
        let _ = w.write_str("> ");
    }
    {
        let mut s = crate::serial::SERIAL.lock();
        let _ = s.write_str("ferrumix> ");
    }
}

/// Read one line from keyboard with echo and backspace support.
/// Blocks until Enter is pressed.  Returns the line length (including trailing \n).
fn read_line(buf: &mut [u8]) -> usize {
    let mut len: usize = 0;
    loop {
        match crate::kb_buffer::try_read_char() {
            Some(0x08) => {
                // Backspace
                if len > 0 {
                    len -= 1;
                    // Erase on screen: move cursor back, write space, move back again
                    let mut w = crate::vga::WRITER.lock();
                    if w.col > 0 {
                        w.col -= 1;
                        w.write_byte(b' ');
                        w.col -= 1;
                    }
                }
            }
            Some(b'\n') => {
                if len < buf.len() {
                    buf[len] = b'\n';
                    len += 1;
                }
                crate::println!(); // newline to screen
                return len;
            }
            Some(c) => {
                if len < buf.len() - 1 {
                    buf[len] = c;
                    len += 1;
                    // Echo to VGA and serial
                    crate::vga::WRITER.lock().write_byte(c);
                    crate::serial::SERIAL.lock().write_byte(c);
                }
            }
            None => {
                // No character available — halt until next interrupt
                unsafe {
                    core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
                }
            }
        }
    }
}

/// Run the interactive shell loop.  Never returns.
pub fn run() -> ! {
    crate::println!("Type 'help' for a list of commands.");
    print_prompt();

    let mut line_buf = [0u8; LINE_MAX];
    loop {
        let len = read_line(&mut line_buf);
        // Strip trailing newline
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
        "ls" => cmd_ls(),
        "ver" | "version" => cmd_version(),
        _ => {
            crate::println!("{}: command not found (type 'help')", cmd);
        }
    }
}

fn cmd_help() {
    crate::println!("Ferrumix shell — available commands:");
    crate::println!("  help      — show this help");
    crate::println!("  clear     — clear the screen");
    crate::println!("  echo TEXT — print text");
    crate::println!("  ps        — list processes");
    crate::println!("  uptime    — system uptime (timer ticks)");
    crate::println!("  mem       — memory statistics");
    crate::println!("  ls        — list devfs devices");
    crate::println!("  uname     — system information");
    crate::println!("  whoami    — current user");
    crate::println!("  version   — kernel version");
    crate::println!("  reboot    — reboot the machine");
}

fn cmd_clear() {
    crate::vga::WRITER.lock().clear();
}

fn cmd_echo(args: &str) {
    crate::println!("{}", args);
}

fn cmd_ps() {
    crate::println!("  PID  PPID  STATE");
    crate::println!("    1     0  Running  (init/shell)");
    crate::println!("{} process(es)", process::process_count());
}

fn cmd_uptime() {
    let ticks = crate::interrupts::get_ticks();
    let seconds = ticks / 100;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    crate::println!(
        "uptime: {}h {}m {}s ({} ticks)",
        hours,
        minutes % 60,
        seconds % 60,
        ticks
    );
}

fn cmd_mem() {
    let (total, used, free) = crate::memory::stats();
    let frame_size_kib = crate::memory::FRAME_SIZE / 1024;
    crate::println!("Memory (4 KiB frames):");
    crate::println!(
        "  total: {} frames ({} MiB)",
        total,
        total * frame_size_kib / 1024
    );
    crate::println!("  used:  {} frames ({} MiB)", used, used * frame_size_kib / 1024);
    crate::println!("  free:  {} frames ({} MiB)", free, free * frame_size_kib / 1024);
}

fn cmd_whoami() {
    crate::println!("root");
}

fn cmd_uname() {
    crate::println!("Ferrumix 0.1.0 x86_64");
}

fn cmd_version() {
    crate::println!("Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust");
    crate::println!("Built with: Rust (no_std, no external crates)");
    crate::println!("Boot: Multiboot2 → 64-bit long mode");
}

fn cmd_ls() {
    crate::println!("/dev:");
    if crate::vfs::find_dev("null").is_some() {
        crate::println!("  null    (char 1:3)");
    }
    if crate::vfs::find_dev("zero").is_some() {
        crate::println!("  zero    (char 1:5)");
    }
    if crate::vfs::find_dev("tty").is_some() {
        crate::println!("  tty     (char 5:0)");
    }
    if crate::vfs::find_dev("ttyS0").is_some() {
        crate::println!("  ttyS0   (char 4:64)");
    }
}

fn cmd_reboot() {
    crate::println!("Rebooting...");
    // PS/2 keyboard controller reset command
    unsafe {
        crate::port::outb(0x64, 0xFE);
    }
    // If that didn't work, triple fault
    unsafe {
        core::arch::asm!("cli");
        let bad_idt: [u8; 10] = [0; 10];
        core::arch::asm!("lidt [{0}]", in(reg) &bad_idt, options(nostack));
        core::arch::asm!("int3", options(nomem, nostack));
    }
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
