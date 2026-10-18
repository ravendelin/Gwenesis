// Only ROM bytes are generated here. All VDP state is populated by the CPU.
pub(crate) fn demo_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x200];
    rom[0..4].copy_from_slice(&0x00ffff00u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
    rom[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    // Display on, plane A at $C000, backdrop 1, H40, increment 2,
    // plane dimensions 64 x 32 tiles.
    for value in [0x8144, 0x8230, 0x8701, 0x8c81, 0x8f02, 0x9001] {
        move_word(&mut rom, value, 0xc00004);
    }
    command(&mut rom, 0x0002, 3); // CRAM palette entries 1..4
    for color in [0x000e, 0x00e0, 0x0e00, 0x0eee] {
        move_word(&mut rom, color, 0xc00000);
    }
    command(&mut rom, 0x0020, 1); // VRAM tiles 1..4, 32 bytes each
    for color in 1..=4 {
        for _ in 0..16 {
            move_word(&mut rom, color * 0x1111, 0xc00000);
        }
    }
    command(&mut rom, 0xc000, 1); // Plane A name table
    for y in 0..32 {
        for x in (0..64).step_by(2) {
            let left = 1 + ((x + y) % 4) as u32;
            let right = 1 + ((x + y + 1) % 4) as u32;
            // MOVE.L writes two successive words through the data port mirrors.
            rom.extend_from_slice(&0x23fcu16.to_be_bytes());
            rom.extend_from_slice(&((left << 16) | right).to_be_bytes());
            rom.extend_from_slice(&0x00c00000u32.to_be_bytes());
        }
    }
    rom.extend_from_slice(&0x60feu16.to_be_bytes()); // BRA -2 forever
    rom
}

fn move_word(rom: &mut Vec<u8>, value: u16, addr: u32) {
    rom.extend_from_slice(&0x33fcu16.to_be_bytes());
    rom.extend_from_slice(&value.to_be_bytes());
    rom.extend_from_slice(&addr.to_be_bytes());
}

fn command(rom: &mut Vec<u8>, addr: u16, code: u8) {
    move_word(rom, (addr & 0x3fff) | (u16::from(code & 3) << 14), 0xc00004);
    move_word(rom, (addr >> 14) | (u16::from(code & 0x3c) << 2), 0xc00004);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        machine::Machine,
        vdp::{H, W},
    };

    #[test]
    fn demo_runs_and_sets_backdrop() {
        let mut m = Machine::new(demo_rom()).unwrap();
        m.run(2000).unwrap();
        // Disable display through MMIO to check the original backdrop behavior.
        m.bus.write16(0xc00004, 0x8104);
        assert!(m.frame().iter().all(|&pixel| pixel == 0xfc0000));
    }

    #[test]
    fn cpu_demo_renders_repeating_four_color_tiles() {
        let mut m = Machine::new(demo_rom()).unwrap();
        m.run(2000).unwrap();
        let frame = m.frame();
        assert_eq!(frame.len(), W * H);
        let colors = [0xfc0000, 0x00fc00, 0x0000fc, 0xfcfcfc];
        for y in 0..H {
            for x in 0..W {
                assert_eq!(
                    frame[y * W + x],
                    colors[(x / 8 + y / 8) % 4],
                    "pixel ({x}, {y})"
                );
            }
        }
        assert_eq!(m.cpu.steps, 2000);
    }
}
