# PWM on ESP32: LEDC vs MCPWM

## Conceptual difference

Both peripherals generate PWM signals but are optimized for different use cases.

### LEDC (LED Control)
- Thinks in **ratios** — you specify what fraction of the period is high (duty cycle %)
- High duty resolution (5–20 bit), frequency is secondary
- Hardware fading between duty values built in
- Low overhead, simple API
- **Use for**: LED dimming, buzzers, fan speed, anything where "% of time on" is what matters

### MCPWM (Motor Control PWM)
- Thinks in **absolute pulse time** — you specify the exact pulse width in timer ticks
- Has fault detection, dead-time insertion, synchronized multi-channel output
- **Use for**: servos, brushed/brushless motors, anything where exact pulse width in µs matters

### How to choose
If you need a specific pulse *width in microseconds* → MCPWM.
If you need a specific *duty cycle percentage* → LEDC.

For servos the position is determined by pulse width (e.g. 1500 µs = 90°), not by duty %.
Using LEDC would require back-calculating the duty % for a given µs at a given frequency,
and any frequency drift would shift the servo position.

---

## How set_timestamp() maps to pulse width

With this configuration:

```rust
PeripheralClockConfig::with_frequency(Rate::from_mhz(1)) // 1 tick = 1 µs
timer_clock_with_frequency(19_999, PwmWorkingMode::Increase, Rate::from_hz(50))
// period = 20000 ticks = 20000 µs = 20 ms = 50 Hz
```

The timer counts **0 → 19999** then resets. With `UP_ACTIVE_HIGH`, the pin goes
**HIGH at count 0** and **LOW when the counter hits the timestamp value**:

```
count:  0                    1500                              19999
        |____________________|__________________________________|
pin:    HIGH                 LOW
        |←── timestamp ────→|
        |←────────────── 20 ms period ────────────────────────→|
```

**timestamp value = pulse width in microseconds** — directly, no conversion needed —
because the 1 MHz clock makes each tick exactly 1 µs.

### Servo example (SG90-style, 50 Hz)
| Angle | Timestamp | Pulse width |
|-------|-----------|-------------|
| 0°    | 550       | 550 µs      |
| 90°   | 1500      | 1500 µs     |
| 180°  | 2500      | 2500 µs     |

> If you used a 2 MHz clock, each tick = 0.5 µs and you'd need `set_timestamp(3000)`
> to get a 1500 µs pulse. The 1 MHz choice keeps the math transparent.
