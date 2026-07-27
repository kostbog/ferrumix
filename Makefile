TARGET := x86_64-unknown-none
BUILD_DIR := target/$(TARGET)/debug
KERNEL := $(BUILD_DIR)/ferrumix
QEMU := qemu-system-x86_64

.PHONY: all build run run-headless clean

all: build

build:
	cargo build --target $(TARGET)

# Graphical run: VGA window + serial on stdio + monitor on stdio.
run: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display gtk -monitor stdio

# Headless run: all output (including VGA mirror) goes to the serial line.
run-headless: build
	$(QEMU) -kernel $(KERNEL) -serial stdio -display none -monitor none

# Functional boot test: run the kernel under headless QEMU and assert that it
# reaches the idle loop. Used by CI (and locally) as the kernel's "test".
test: build
	@out=$$(timeout 15 $(QEMU) -kernel $(KERNEL) -serial stdio -display none -monitor none 2>&1); \
	printf '%s\n' "$$out"; \
	echo "$$out" | grep -q "Ferrumix 0.1.0" || { echo "FAIL: banner not found"; exit 1; }; \
	echo "$$out" | grep -q "is alive" || { echo "FAIL: kernel did not reach idle loop"; exit 1; }; \
	echo "BOOT TEST PASSED"

clean:
	cargo clean
