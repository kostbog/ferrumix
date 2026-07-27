# Ferrumix

> *ferrum* (лат. «железо») + *ix* — клон Unix, написанный на Rust.

Ferrumix — это с нуля написанное **ядро ОС для x86_64**, загружаемое по
спецификации **Multiboot2** и запускаемое в QEMU. Сейчас реализован базовый,
но реально работающий каркас ядра (приоритет по договорённости), поверх
которого позже появятся процессы в ring 3, системные вызовы, файловая
система и оболочка.

> ⚠️ **Заметка о сборке в этом окружении.** Песочница, где писался код, не
> имеет Rust-тулчейна и не имеет доступа к зеркалам Rust/apt (прокси
> пропускает только `github.com`, `pypi.org`, `registry.npmjs.org`). Поэтому
> код **не компилировался и не запускался здесь** — он написан и выверен вручную,
> и готов к сборке у тебя на машине (см. «Сборка и запуск»).

---

## Что уже работает

- 32→64-битный trampoline и включение long mode (paging) — `src/boot.S`
- Multiboot2-заголовок, загрузка через `qemu-system-x86_64 -kernel`
- Текстовый VGA-драйвер (0xB8000) + зеркалирование в последовательный порт
- GDT + TSS (с IST-стеком для double fault)
- IDT со всеми исключениями, double fault и page fault
- 8259 PIC (ремап на векторы 0x20+), PIT-таймер (~100 Гц), клавиатура (set 1)
- Парсер Multiboot2 memory map (считаем доступную RAM)
- `panic_handler`, idle-цикл с `hlt` и включёнными прерываниями

**Зависимости:** ни одного внешнего крейта. Только `core` + стабильный Rust
(`asm!`, `global_asm!`, `extern "x86-interrupt"`).

## Сборка и запуск

Требуется: Rust (stable, ≥ 1.69), таргет `x86_64-unknown-none`, QEMU.

```bash
# 1. Установить таргет и (при необходимости) rust-lld
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview   # даёт rust-lld, если линковщик его требует

# 2. Собрать ядро
cargo build --target x86_64-unknown-none
#    или просто `make build` (цель в Makefile уже задаёт --target)

# 3. Запустить в QEMU
#    - графический режим (окно VGA + serial в терминале):
make run
#    - headless (весь вывод, включая VGA, идёт в serial -> терминал):
make run-headless
```

Если линковщик жалуется, что не найден `rust-lld`/`ld`, добавь
`llvm-tools-preview` (см. выше). Если `cargo build` падает с ошибкой про
`linker`, убедись, что таргет `x86_64-unknown-none` установлен.

Ожидаемый вывод (в serial / терминале):

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

## Структура

```
ferrumix/
├── Cargo.toml            # пакет ядра, профили panic="abort", без зависимостей
├── .cargo/config.toml    # таргет x86_64-unknown-none + линковщик linker.ld
├── linker.ld             # раскладка: ядро по адресу 1 MiB, Multiboot2-заголовок спереди
├── Makefile              # build / run / run-headless / clean
├── src/
│   ├── main.rs           # точка входа kernel_main(magic, mb_info) + panic_handler
│   ├── boot.rs           # подключает boot.S через global_asm!
│   ├── boot.S            # Multiboot2-заголовок + trampoline в long mode
│   ├── port.rs           # port I/O (inb/outb/...), hlt/sti/cli
│   ├── spinlock.rs       # мини-спинлок на атомиках
│   ├── vga.rs            # текстовый VGA-драйвер + print!/println!
│   ├── serial.rs         # COM1 + serial_print!/serial_println!
│   ├── gdt.rs            # GDT + TSS (IST)
│   ├── idt.rs            # дескрипторы IDT + lidt
│   ├── interrupts.rs     # обработчики, PIC, PIT, клавиатура
│   └── multiboot.rs      # парсер Multiboot2 memory map
├── userspace/            # каркас ring-3 программ (пока не подключён к ядру)
│   ├── src/main.rs
│   └── src/syscall.rs    # черновик ABI системных вызовов (int 0x80)
└── docs/ROADMAP.md       # план развития в полноценный клон Unix
```

## Как это загружается (коротко)

1. QEMU с `-kernel` видит Multiboot2-заголовок в `boot.S`, грузит ядро в
   32-битный protected mode и прыгает на `_start`, передавая в `EAX` магик,
   а в `EBX` — указатель на структуру Multiboot2.
2. `_start` строит таблицы страниц (identity-map первого 1 GiB 2 MiB-страницами),
   включает PAE/long mode/paging и через `lgdt` + far-jump переходит в 64-бит.
3. В long mode настраивается стек и вызывается `kernel_main(magic, mb_info)`.
4. Ядро инициализирует VGA/serial, GDT/TSS, IDT/PIC/PIT и уходит в idle с
   `hlt`, обрабатывая таймер и клавиатуру.

## Следующие шаги

См. [`docs/ROADMAP.md`](docs/ROADMAP.md): процессы в ring 3, системные
вызовы, виртуальная файловая система, оболочка и утилиты (`ls`, `cat`, …).
