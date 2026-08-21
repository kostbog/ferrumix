TARGET := x86_64-unknown-none
CUSTOM_TARGET := x86_64-ferrumix.json
BUILD_DIR := target/$(TARGET)/debug
RELEASE_DIR := target/$(TARGET)/release
KERNEL := $(BUILD_DIR)/ferrumix
KERNEL_RELEASE := $(RELEASE_DIR)/ferrumix
QEMU := qemu-system-x86_64

.PHONY: all build build-release build-custom run run-headless clean test test-release fmt clippy ci

all: build

# ── Build ──────────────────────────────────────────────────────────────

build:
	cargo build --target $(TARGET)

build-release:
	cargo build --target $(TARGET) --release

build-custom:
	cargo +nightly build --target $(CUSTOM_TARGET) -Zbuild-std=core,compiler_builtins
	cargo +nightly build --target $(CUSTOM_TARGET) -Zbuild-std=core,compiler_builtins --release || true

# ── Run ────────────────────────────────────────────────────────────────

run: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display gtk -monitor stdio

run-headless: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display none -monitor none

# ── Lint ───────────────────────────────────────────────────────────────

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --target $(TARGET) -- -D warnings || true

# ── Test ───────────────────────────────────────────────────────────────

# Full test suite: lint + build + boot test.  This is the single entry
# point used by CI (`make test`).  Run it locally to verify everything.

test: fmt clippy build _boot-test-debug _custom-target-diag

_custom-target-diag:
	@bash ci-custom.sh

test-release: fmt clippy build-release _boot-test-release

# Internal targets — run the kernel under headless QEMU and assert that it
# reaches the idle loop, Unix subsystems initialised, and the shell prompt.

_boot-test-debug:
	@out=$$(timeout 20 $(QEMU) -kernel $(KERNEL) -serial stdio -display none -monitor none 2>&1); \
	printf '%s\n' "$$out"; \
	echo "$$out" | grep -q "Ferrumix 0.1.0" || { echo "FAIL: banner not found"; exit 1; }; \
	echo "$$out" | grep -q "is alive" || { echo "FAIL: kernel did not reach idle loop"; exit 1; }; \
	echo "$$out" | grep -q "GDT" || { echo "FAIL: GDT not initialized"; exit 1; }; \
	echo "$$out" | grep -q "IDT" || { echo "FAIL: IDT not initialized"; exit 1; }; \
	echo "$$out" | grep -q "frame allocator" || { echo "FAIL: frame allocator missing"; exit 1; }; \
	echo "$$out" | grep -q "paging" || { echo "FAIL: paging missing"; exit 1; }; \
	echo "$$out" | grep -q "syscall" || { echo "FAIL: syscall gate missing"; exit 1; }; \
	echo "$$out" | grep -q "process" || { echo "FAIL: process table missing"; exit 1; }; \
	echo "$$out" | grep -q "VFS" || { echo "FAIL: VFS missing"; exit 1; }; \
	echo "$$out" | grep -q "console: text output ready" || { echo "FAIL: console missing"; exit 1; }; \
	echo "$$out" | grep -q "paging: self-test OK" || { echo "FAIL: paging self-test failed"; exit 1; }; \
	echo "$$out" | grep -q "syscall: self-test" || { echo "FAIL: syscall self-test missing"; exit 1; }; \
	echo "$$out" | grep -q "elf: hello loaded" || { echo "FAIL: ELF loader did not run"; exit 1; }; \
	echo "$$out" | grep -q "hello from ring 3" || { echo "FAIL: ring 3 program did not run"; exit 1; }; \
	echo "$$out" | grep -q "exited with status 7" || { echo "FAIL: ring 3 exit status wrong"; exit 1; }; \
	echo "$$out" | grep -q "ferrumix>" || { echo "FAIL: shell prompt not found"; exit 1; }; \
	echo "BOOT TEST debug PASSED (lint + build + unix subsystems + ring 3 + shell)"

_boot-test-release:
	@out=$$(timeout 20 $(QEMU) -kernel $(KERNEL_RELEASE) -serial stdio -display none -monitor none 2>&1); \
	printf '%s\n' "$$out"; \
	echo "$$out" | grep -q "Ferrumix 0.1.0" || { echo "FAIL: banner not found"; exit 1; }; \
	echo "$$out" | grep -q "is alive" || { echo "FAIL: idle loop not reached"; exit 1; }; \
	echo "$$out" | grep -q "hello from ring 3" || { echo "FAIL: ring 3 program did not run"; exit 1; }; \
	echo "$$out" | grep -q "ferrumix>" || { echo "FAIL: shell prompt not found"; exit 1; }; \
	echo "BOOT TEST release PASSED"

# ── Clean ──────────────────────────────────────────────────────────────

clean:
	cargo clean
