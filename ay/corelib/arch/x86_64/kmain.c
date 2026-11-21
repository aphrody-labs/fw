// C entry point of the bare-metal image: 16550 serial console on COM1 and the QEMU exit device.
#include "core.h"

enum : uint16_t { COM1 = 0x3F8, DEBUG_EXIT = 0xF4 };

static void outb(uint16_t port, uint8_t value) { __asm__ volatile("outb %0, %1" : : "a"(value), "Nd"(port)); }

static uint8_t inb(uint16_t port) {
  uint8_t value;
  __asm__ volatile("inb %1, %0" : "=a"(value) : "Nd"(port));
  return value;
}

/// 115200 baud, 8 data bits, no parity, one stop bit, FIFO on.
static void serial_init(void) {
  outb(COM1 + 1, 0x00); // interrupts off
  outb(COM1 + 3, 0x80); // DLAB on
  outb(COM1, 0x01);     // divisor 1 = 115200 baud
  outb(COM1 + 1, 0x00);
  outb(COM1 + 3, 0x03); // 8N1, DLAB off
  outb(COM1 + 2, 0xC7); // enable and clear FIFOs
  outb(COM1 + 4, 0x0B); // DTR, RTS, OUT2
}

static void serial_put(void *, uint8_t byte) {
  while (!(inb(COM1 + 5) & 0x20)) {
  }
  outb(COM1, byte);
}

[[noreturn]] static void halt(void) {
  for (;;) __asm__ volatile("hlt");
}

/// Run the shared demo on COM1 and tell QEMU the verdict through isa-debug-exit: status (code << 1) | 1,
/// so 0x10 gives 33 (pass) and 0x11 gives 35 (fail). On real hardware the write does nothing and we halt.
[[noreturn]] void kmain(void) {
  serial_init();
  const ay_console com1 = {serial_put, nullptr};
  outb(DEBUG_EXIT, ay_run_reference(&com1) ? 0x10 : 0x11);
  halt();
}
