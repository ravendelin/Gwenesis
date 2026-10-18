# Gwenesis — educational Sega Genesis / Mega Drive emulator

This is an **early prototype, not a commercial-game-compatible emulator**. It implements a small Motorola 68000 interpreter, portions of the 24-bit memory map, basic VDP register/control/data writes and a simplified 320×224 tile renderer. No audio, Z80, sprites, scrolling, proper timing, DMA, interrupts, or complete CPU instruction set yet.

## Run (Rust / Cargo)

```bash
cargo test
cargo run -- --demo --ppm frame.ppm
cargo run -- path/to/your_legal_homebrew.bin --steps 50000 --ppm output.ppm
```

`frame.ppm` is a standard RGB image (open with GIMP/ImageMagick). The built-in demo ROM executes real 68000 instructions to configure the VDP and uploads four solid-color tiles, a red/green/blue/white palette, and a 64×32 plane A name table through VDP MMIO to produce a repeating colored tile pattern.

For a realtime window (needs system graphical libraries, notably X11/Wayland on Linux):

```bash
cargo run --features window -- --demo --window
```

Press Esc to quit. An unsupported opcode halts CPU execution and logs an error, but keeps the last frame visible in window mode.

## Implemented

- ROM loading up to 4 MiB; reset SSP/PC vectors
- Big-endian RAM mapped at `$FF0000–$FFFFFF`
- Partial 68000: NOP, RTS, JMP/JSR absolute long, MOVE.W/L/B immediate→absolute long, MOVE.W/L immediate→Dn, MOVEQ, BRA, BSR
- Partial VDP: control commands, registers, VRAM/CRAM/VSRAM reads and writes, palette, backdrop, simple plane A tiles
- Basic system status and controller stub, PPM frame export, unit tests

## Known limitations

Existing Sega Genesis games **will not run** in general. No cycle accuracy; most 68000 instructions, I/O ports, interrupts, sprites, sound and save games are missing. Plane size is hardcoded, and VDP tile renderer ignores priority and scroll values. This prototype is intended as a starting point for an incremental emulator implementation.

## Suggested milestones

1. Full 68000 effective addressing and arithmetic/flags, exception vectors, interrupts.
2. Complete VDP modes 4/5: tiles, planes, scrolling, sprites, DMA and scanline timing.
3. Input state/controller protocol; Z80 bus, PSG and YM2612 audio.
4. Cartridge save RAM, region timing, compatibility tests using freely distributable homebrew test ROMs.

## Core layout

`Machine` owns `Cpu` and `Bus`; `Bus` owns ROM, RAM and `Vdp`.
`cpu.rs` executes instructions using bus accesses, `vdp.rs` renders memory-backed
plane A tiles, and `demo.rs` builds instruction bytes without touching VDP state.
`main.rs` provides the CLI, PPM export and optional Gwenesis window.
The demo finishes its setup in 1104 instructions; the default 2000-step run
then remains in its final `BRA -2` loop. Output is 320×224 pixels.

VDP reads are synchronous and simplified (no hardware prefetch/FIFO timing).
The renderer uses a fixed 64×32 plane A and the existing 3-bit-to-RGB mapping
(maximum channel value 252). CPU behavior outside this task remains incomplete:
word-displacement branches use the post-extension PC, immediate stores do not
update condition codes, and MOVEQ decoding accepts reserved bit-8 encodings.
