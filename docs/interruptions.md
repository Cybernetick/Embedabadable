# GPIO Interrupts in esp-hal (Rust, bare-metal)

## Two separate registrations

```rust
io.set_interrupt_handler(handler);  // tells CPU: "when ANY GPIO interrupt fires, jump to `handler`"
button.listen(Event::FallingEdge);  // tells GPIO hardware: "fire an interrupt when GPIO0 falls"
```

- `listen` configures the **GPIO peripheral's interrupt registers** — sets the trigger type for a specific pin in hardware
- `set_interrupt_handler` configures the **CPU's interrupt vector table** — sets which function to jump to when the GPIO peripheral signals the CPU

## What happens on button press

```
GPIO0 pin voltage falls
    → GPIO peripheral detects falling edge on GPIO0
    → GPIO peripheral raises its interrupt line to the CPU
    → CPU saves registers, jumps to your `handler` function
    → your handler runs
```

## Status register

The GPIO peripheral has a **status register** — a bitmask where each bit represents one pin.
When GPIO0 fires, bit 0 is set.

```rust
button.is_interrupt_set()   // reads:  "is bit 0 set in GPIO_STATUS_REG?"
button.clear_interrupt()    // writes: "clear bit 0 in GPIO_STATUS_REG"
```

If you don't call `clear_interrupt()`, the interrupt fires again immediately after the handler returns.

## One handler for all GPIO interrupts

There is one handler for **all** GPIO interrupts. Your handler is responsible for:
1. Checking which pin(s) fired — `is_interrupt_set()`
2. Doing the work
3. Clearing the flag — `clear_interrupt()`

This is exactly what ESP-IDF's `gpio_install_isr_service` does internally — it installs one shared
handler and dispatches to per-pin callbacks. In esp-hal you write that dispatcher yourself.

## Full picture

```
Hardware:  [GPIO0 pin] --falling edge--> [GPIO peripheral status register bit 0]
                                                        |
                                              GPIO peripheral raises interrupt line
                                                        |
CPU:                                         jumps to handler() function
                                                        |
Your code:                           is_interrupt_set() reads status register
                                     clear_interrupt()  clears status register
```

## Why keep the `Input` handle alive?

The `Input` struct **is** the pin's configuration and state in esp-hal — there's no background driver
managing it. You need it in the handler to call `is_interrupt_set()` and `clear_interrupt()`.
Standard pattern: move it into a `Mutex<RefCell<Option<Input>>>` static.

```rust
static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));

// in main — move button into the static:
critical_section::with(|cs| {
    button.listen(Event::FallingEdge);
    BUTTON.borrow_ref_mut(cs).replace(button);
});

// in handler:
critical_section::with(|cs| {
    let mut b = BUTTON.borrow_ref_mut(cs);
    let b = b.as_mut().unwrap();
    if b.is_interrupt_set() {
        // handle it
        b.clear_interrupt();
    }
});
```

## Sharing state between handler and main

| Use case | Tool |
|---|---|
| Simple flag / 2-state toggle | `AtomicBool` |
| Cycling through N patterns | `AtomicU8` with `fetch_update` |
| Complex struct / peripheral | `Mutex<RefCell<T>>` via `critical_section` |

### AtomicU8 pattern cycle example

```rust
use core::sync::atomic::{AtomicU8, Ordering};

const NUM_PATTERNS: u8 = 3;
static PATTERN: AtomicU8 = AtomicU8::new(0);

// in handler:
PATTERN.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |p| {
    Some((p + 1) % NUM_PATTERNS)
}).ok();

// in main:
match PATTERN.load(Ordering::Relaxed) {
    0 => police_lights(...),
    1 => flashing(...),
    2 => snake(...),
    _ => unreachable!(),
}
```

## esp-hal vs ESP-IDF comparison

| | ESP-IDF (C) | esp-hal (Rust, bare-metal) |
|---|---|---|
| Interrupt dispatcher | Installed by `gpio_install_isr_service` | You write it |
| Clear interrupt flag | Done internally by the driver | You call `clear_interrupt()` |
| Pin identity in ISR | Raw integer passed via `arg` | Must hold the `Input` handle |
| Pin state management | ESP-IDF GPIO driver | The `Input` struct itself |
| OS / scheduler | FreeRTOS | None |
