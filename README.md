# Rhythm PI

Rhythm PI is a rhythm game built with **Rust** and **Slint**, designed to run efficiently on a Raspberry Pi Zero 2W and all other platforms. The main goal is to have it run, optimizations will happen closer to the time of when i have the Raspberry Pi.

## Platform Targets

| Platform | Target | Status | Binary Size |
|----------|--------|--------|-------------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | | ~MB |
| Windows x86_64 | `x86_64-pc-windows-gnu` | | ~MB |
| **Raspberry Pi Zero 2W** | `armv7-unknown-linux-gnueabihf` | | ~MB |

## Raspberry Pi Zero 2W Target

**Specs:**
- Broadcom BCM2710A1, quad-core 64-bit (ARM Cortex-A53 @ 1GHz)
- 512 MB LPDDR2 SDRAM
- OpenGL ES 1.1, 2.0 graphics

**Resource Constraints:**
- RAM: Game runs in <200MB
- CPU: Efficient async I/O, minimal blocking
- Storage: ~200MB for binary
- Audio: Optimized streaming over WebSockets

## Roadmap

### Phase 1: UI Foundation
- [x] Slint-based UI
- [x] Three main screens
- [x] Cross-platform support
- [x] WebSocket client

### Phase 2: Gameplay
- [x] Note detection
- [x] Scoring system
- [x] Combo counter
- [x] Chart parsing

### Phase 3: Optimization
- [ ] Memory optimization for Pi Zero
- [ ] CPU profiling and tuning
- [ ] Binary size reduction
- [ ] Audio streaming optimization

### Phase 4: Features
- [x] Song library management
- [ ] Multiplayer support
- [ ] Leaderboards
- [ ] Custom charts

## License

MIT License - see LICENSE file for details

---