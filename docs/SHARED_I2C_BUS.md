# Sharing One I2C Bus: StaticCell + Mutex

How `src/bin/i2c_oled.rs` shares a single I2C bus between `main`, the `read_time` task and
(later) the OLED drawing task, and what each piece of this type contributes:

```rust
static CELL: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> = StaticCell::new();
let shared = CELL.init(Mutex::new(i2c_bus.into_async()));
```

## The Problem

There is **one** hardware I2C peripheral (`I2C0`) and **one** driver object (`I2c<Async>`),
but several pieces of code need it:

- `main`: OLED init, `is_time_set`, `set_time`
- the `read_time` task, every second
- later, `draw_clock_time`

Read the type from the inside out. Each layer answers a different question:

```
StaticCell< Mutex< NoopRawMutex, I2c<'static, Async> > >
└ where it ┘ └ who may use ┘ └ how the ┘ └ the driver ┘
   lives       it right now    mutex
                               protects
                               itself
```

## Layer 1: `I2c<'static, Async>`, the Driver

- **Every operation needs `&mut self`.** `write_async` and `write_read_async` change hardware
  state (FIFOs, status registers). Rust allows only one `&mut` at a time, so you can't hand
  plain `&mut I2c` to several tasks.
- **`Async`**: a transfer waits by `.await`ing an interrupt instead of spinning on the CPU,
  so other tasks can run during a 1025-byte screen clear.
- **`'static`**: the driver *owns* the `I2C0`, GPIO8 and GPIO9 peripheral handles, so it can
  live forever.

## Layer 2: `Mutex<…>`, Taking Turns

Anyone holding a shared `&Mutex` can call `.lock().await` to get a **guard**, which acts as
`&mut I2c`. Only one guard exists at a time.

What it contributes:

1. **The borrow checker accepts it.** You hand out cheap shared `&Mutex` references, and the
   mutex enforces the "only one `&mut`" rule while the program runs.
2. **Multi-step operations stay together** (the more important point). Many operations are
   several I2C transactions that must not be interleaved:
   - OLED: set columns → set pages → write data
   - DS1307: write register pointer `0x00` → read 7 bytes

   If `read_time` slipped its transaction between "set columns" and "write data", pixels
   would land in the wrong place. Holding the guard for the whole sequence prevents that.

**Why `embassy_sync::mutex::Mutex` and not a blocking mutex:** if the lock is taken,
`.lock().await` puts the task to sleep and lets the executor run other tasks. A blocking
mutex would spin, and with one executor on one core the holder could never run to release
it, so the program would **deadlock**.

**The guard releases the lock when it is dropped.** That's why `read_time` calls
`drop(bus_)` *before* `Timer::after_secs(1)`. Otherwise the guard lives to the end of the
loop body, and the bus stays locked through the whole sleep. Embassy's mutex doesn't queue
waiters fairly, so other tasks could wait forever.

## Layer 3: `NoopRawMutex`, How the Mutex Protects Itself

The async mutex has its own small internal state (a "locked" flag and a list of waiting
tasks). The `RawMutex` parameter decides how that state is protected during the few
instructions it takes to update it:

| RawMutex                  | How it protects                                         | When it's valid                                                    |
|---------------------------|---------------------------------------------------------|--------------------------------------------------------------------|
| `NoopRawMutex`            | It doesn't. Costs nothing                               | All users in the **same executor, same core**, no interrupt handlers |
| `CriticalSectionRawMutex` | Briefly disables interrupts (plus a cross-core lock)    | Multiple executors, both ESP32-S3 cores, or interrupt handlers     |

All users here run in the single `esp_rtos::main` executor. Only one task runs at a time,
and it only switches at `.await` points, so no two tasks can touch the internal state at
once. **Noop is correct.**

The compiler enforces this: `Mutex<NoopRawMutex, _>` is not `Sync`, so it can't be passed
to a second core's executor by accident.

Compare `CURRENT_TIME_READINGS` in `lib.rs`, which uses `CriticalSectionRawMutex`. It's a
plain `static` with no restriction on who uses it, so the safer, more general choice fits.

## Layer 4: `StaticCell`, a Place to Live Forever

The mutex must live somewhere every user can reach for as long as they need it. Two rules
make that hard:

1. **Embassy tasks require `'static` arguments.** A task is stored in a static pool and can
   outlive the function that spawned it. `main` actually *returns* right after
   `spawner.spawn(...)` while `read_time` keeps running.
2. **A plain `static` can't hold it.** `static BUS: Mutex<…> = …` needs a value computed at
   compile time, but `I2c` only exists after `esp_hal::init()` hands out the peripherals at
   run time.

You need storage that is **static (lives forever) but filled in once at run time**. That is
exactly what `StaticCell` is:

- `static CELL: StaticCell<…> = StaticCell::new();` reserves empty memory at compile time,
  which is allowed because "empty" is a constant.
- `CELL.init(value)` moves the value in at run time and returns `&'static mut`.
- Calling `init` a second time **panics**, so you can never get two `&'static mut` to the
  same memory.

The returned `&'static mut Mutex<…>` is quietly turned into a shared `&'static Mutex<…>`
(`shared`). Shared references can be copied freely, so the same `shared` goes to
`run_init_sequence`, `is_time_set`, `set_time` and `read_time`.

Alternatives are worse on bare metal:

- `Box::leak` needs a heap allocator, which isn't set up.
- `static mut` needs `unsafe` and gives no protection against creating two `&mut`.

## The Whole Flow

```
compile time:  CELL = [ empty slot in .bss ]

runtime:       esp_hal::init() ──► I2c::new(I2C0, GPIO8, GPIO9).into_async()
                                        │
                     Mutex::new(...) ◄──┘
                           │
               CELL.init(...) ──► &'static Mutex  ("shared")
                                        │  (copied freely)
               ┌──────────────┬─────────┼──────────────┐
          main: OLED init  main: RTC  read_time task  (future) draw task
               │              │         │              │
               └── each: lock().await → use &mut I2c → drop guard ──┘
```

## What Breaks if You Remove Each Piece

| Remove                                     | Result                                                                   |
|--------------------------------------------|--------------------------------------------------------------------------|
| `StaticCell` (local in `main`)             | Compile error: borrowed value doesn't live long enough for `spawn`       |
| `Mutex` (share `&I2c`)                     | Compile error: can't call `&mut self` methods through `&`                |
| async `Mutex` → blocking mutex             | Deadlock as soon as two tasks compete for the bus                        |
| `NoopRawMutex` → `CriticalSectionRawMutex` | Still works, with a tiny extra cost per lock. Only needed with multiple executors, both cores or interrupt handlers |
| `Async` → `Blocking`                       | Still works, but a 1 KB transfer stops every task until it finishes      |

## Why Passing the Local `shared` Is Safe

`shared` is a local variable in `main`, and it disappears when `main` returns. So why can
tasks keep using it?

Because `shared` is not the mutex. It is a **reference**: a 4-byte value holding an address.
The variable and the thing it points to have different lifetimes:

```
main's stack (temporary)           static memory (.bss, lives forever)
┌──────────────────────┐           ┌────────────────────────────────┐
│ shared: 0x3FC8_1234 ─┼──────────►│ CELL: Mutex<NoopRawMutex, I2c> │
└──────────────────────┘           └────────────────────────────────┘
  dies with main                     filled by CELL.init(), never freed
```

- The **variable** `shared` lives on `main`'s stack and does go away.
- The **mutex** it points to was moved into `CELL` by `init`. It never lived on `main`'s
  stack.

The `'static` in `&'static Mutex<…>` is a promise about the **pointee** (the mutex exists
for the rest of the program), not about the variable holding the address.

**Passing it copies the address.** `&T` is `Copy`, so `read_time(shared)` copies the address
into the task's own storage, which is in embassy's static task pool. After the spawn there
are two independent copies of the same address. When `main` ends, its copy disappears, and
the task's copy still points at the mutex in `CELL`, which is still there.

Think of a phone number written on a sticky note: throw the original note away after
photocopying it, and the phone still works.

### What *would* be unsafe

```rust
let bus = Mutex::new(i2c_bus.into_async());   // mutex lives on main's stack
spawner.spawn(read_time(&bus).unwrap());       // ❌ compile error
```

Here the pointee itself is on `main`'s stack. Once `main` returns, that memory is reused,
and the task would read garbage. The compiler rejects it with "`bus` does not live long
enough", because `&bus` is only valid while `main` runs and the task asks for `&'static`.

**Rule:** the compiler checks the lifetime of what a reference *points to*, not of the
variable holding it. `StaticCell` turns a value created at run time into something whose
address is valid for `'static`. After that, copies of the address can go anywhere.

## Next Step: `I2cDevice`

As the OLED and RTC drivers grow, the usual pattern is
`embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice`. It wraps the same
`&'static Mutex<…, I2c>`, and each driver gets its own "device" handle that locks the bus
for each transaction automatically. It's the same structure as now, with the locking moved
out of your driver code.
