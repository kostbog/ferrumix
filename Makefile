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

clean:
	cargo clean
