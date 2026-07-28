TARGET := x86_64-unknown-none
CUSTOM_TARGET := x86_64-ferrumix.json
BUILD_DIR := target/$(TARGET)/debug
KERNEL := $(BUILD_DIR)/ferrumix
QEMU := qemu-system-x86_64

.PHONY: all build build-release build-custom run run-headless clean test test-release fmt clippy

all: build

build:
	cargo build --target $(TARGET)

build-release:
	cargo build --target $(TARGET) --release

# Custom Unix target (nightly + build-std)
build-custom:
	cargo +nightly build --target $(CUSTOM_TARGET) -Zbuild-std=core,compiler_builtins
	cargo +nightly build --target $(CUSTOM_TARGET) -Zbuild-std=core,compiler_builtins --release || true

# Graphical run: VGA window + serial on stdio + monitor on stdio.
run: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display gtk -monitor stdio

# Headless run: all output (including VGA mirror) goes to the serial line.
run-headless: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display none -monitor none

# Functional boot test: run the kernel under headless QEMU and assert that it
# reaches the idle loop AND Unix subsystems initialized. Used by CI.
# This is the authoritative test — all builds/tests run on GitHub, this target
# merely mirrors CI locally.
test: build
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
	echo "BOOT TEST PASSED (Unix subsystems verified)"

test-release: build-release
	@out=$$(timeout 20 $(QEMU) -kernel target/$(TARGET)/release/ferrumix -serial stdio -display none -monitor none 2>&1); \
	printf '%s\n' "$$out"; \
	echo "$$out" | grep -q "Ferrumix 0.1.0" || { echo "FAIL: banner not found"; exit 1; }; \
	echo "$$out" | grep -q "is alive" || { echo "FAIL: idle loop not reached"; exit 1; }; \
	echo "BOOT TEST RELEASE PASSED"

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --target $(TARGET) -- -D warnings || true

clean:
	cargo clean
