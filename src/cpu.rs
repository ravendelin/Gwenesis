use crate::bus::Bus;

pub(crate) struct Cpu {
    d: [u32; 8],
    a: [u32; 8],
    pub(crate) pc: u32,
    sr: u16,
    pub(crate) steps: u64,
}
impl Cpu {
    pub(crate) fn reset(bus: &mut Bus) -> Result<Self, String> {
        let mut cpu = Self {
            d: [0; 8],
            a: [0; 8],
            pc: bus.read32(4),
            sr: 0x2700,
            steps: 0,
        };
        cpu.a[7] = bus.read32(0);
        if !bus.contains_rom_address(cpu.pc) {
            return Err(format!("Invalid reset PC ${:06X}", cpu.pc));
        }
        Ok(cpu)
    }
    fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let value = bus.read16(self.pc);
        self.pc = self.pc.wrapping_add(2);
        value
    }
    fn fetch32(&mut self, bus: &mut Bus) -> u32 {
        let value = self.fetch16(bus) as u32;
        (value << 16) | self.fetch16(bus) as u32
    }
    fn set_nz(&mut self, val: u32, bits: u32) {
        self.sr &= !0x0f;
        if val & (1u32 << (bits - 1)) != 0 {
            self.sr |= 8;
        }
        if val
            & (if bits == 32 {
                u32::MAX
            } else {
                (1 << bits) - 1
            })
            == 0
        {
            self.sr |= 4;
        }
    }
    pub(crate) fn step(&mut self, bus: &mut Bus) -> Result<(), String> {
        let at = self.pc;
        let op = self.fetch16(bus);
        match op {
            0x4e71 => {} // NOP
            0x4e75 => {
                self.pc = bus.read32(self.a[7]);
                self.a[7] = self.a[7].wrapping_add(4);
            } // RTS
            0x4ef9 => {
                self.pc = self.fetch32(bus);
            } // JMP abs.l
            0x4eb9 => {
                let dest = self.fetch32(bus);
                self.a[7] = self.a[7].wrapping_sub(4);
                bus.write32(self.a[7], self.pc);
                self.pc = dest;
            } // JSR
            0x33fc => {
                let val = self.fetch16(bus);
                let addr = self.fetch32(bus);
                bus.write16(addr, val);
            } // MOVE.W #imm,abs.l
            0x23fc => {
                let val = self.fetch32(bus);
                let addr = self.fetch32(bus);
                bus.write32(addr, val);
            } // MOVE.L #imm,abs.l
            0x13fc => {
                let val = self.fetch16(bus) as u8;
                let addr = self.fetch32(bus);
                bus.write8(addr, val);
            }
            0x303c..=0x3e3c if op & 0x01ff == 0x003c => {
                let reg = ((op >> 9) & 7) as usize;
                let val = self.fetch16(bus);
                self.d[reg] = (self.d[reg] & 0xffff0000) | val as u32;
                self.set_nz(val as u32, 16);
            }
            0x203c..=0x2e3c if op & 0x01ff == 0x003c => {
                let reg = ((op >> 9) & 7) as usize;
                let val = self.fetch32(bus);
                self.d[reg] = val;
                self.set_nz(val, 32);
            }
            0x7000..=0x7fff => {
                let reg = ((op >> 9) & 7) as usize;
                let val = (op as u8 as i8 as i32) as u32;
                self.d[reg] = val;
                self.set_nz(val, 32);
            } // MOVEQ
            0x6000..=0x60ff => {
                let disp = if op & 0xff == 0 {
                    self.fetch16(bus) as i16 as i32
                } else {
                    (op as u8 as i8) as i32
                };
                self.pc = self.pc.wrapping_add_signed(disp);
            } // BRA
            0x6100..=0x61ff => {
                let disp = if op & 0xff == 0 {
                    self.fetch16(bus) as i16 as i32
                } else {
                    (op as u8 as i8) as i32
                };
                self.a[7] = self.a[7].wrapping_sub(4);
                bus.write32(self.a[7], self.pc);
                self.pc = self.pc.wrapping_add_signed(disp);
            } // BSR
            _ => return Err(format!("Unsupported 68000 opcode ${op:04X} at ${at:06X}")),
        }
        self.steps += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{demo::demo_rom, machine::Machine};

    #[test]
    fn reset_loads_ssp_pc_and_sr() {
        let m = Machine::new(demo_rom()).unwrap();
        assert_eq!(m.cpu.a[7], 0xffff00);
        assert_eq!(m.cpu.pc, 0x200);
        assert_eq!(m.cpu.sr, 0x2700);
        assert_eq!(m.cpu.d, [0; 8]);
        let mut rom = demo_rom();
        rom[4..8].copy_from_slice(&0x400000u32.to_be_bytes());
        assert!(Machine::new(rom)
            .err()
            .unwrap()
            .contains("Invalid reset PC"));
    }

    #[test]
    fn unknown_opcode_fails_clearly() {
        for opcode in [0xffffu16, 0x303d] {
            let mut rom = demo_rom();
            rom[0x200..0x202].copy_from_slice(&opcode.to_be_bytes());
            let mut m = Machine::new(rom).unwrap();
            assert_eq!(
                m.step().unwrap_err(),
                format!("Unsupported 68000 opcode ${opcode:04X} at $000200")
            );
            assert_eq!(m.cpu.steps, 0);
        }
    }

    #[test]
    fn cpu_writes_reach_vdp_through_bus() {
        let mut m = Machine::new(demo_rom()).unwrap();
        m.run(12).unwrap(); // Six registers, two command words, four palette colors.
        m.bus.write16(0xc00004, 0x0002); // CRAM read address 2, code 8
        m.bus.write16(0xc00004, 0x0020);
        for color in [0x000e, 0x00e0, 0x0e00, 0x0eee] {
            assert_eq!(m.bus.read16(0xc00000), color);
        }
    }
    #[test]
    fn retained_move_instructions_and_flags() {
        let mut rom = demo_rom();
        let words: [u16; 24] = [
            0x4e71, // NOP
            0x70ff, // MOVEQ #-1,D0
            0x303c, 0, // MOVE.W #0,D0 preserves upper half
            0x223c, 0x1234, 0x5678, // MOVE.L #$12345678,D1
            0x13fc, 0x00ab, 0x00ff, 0x0100, 0x33fc, 0xcdef, 0x00ff, 0x0102, 0x23fc, 0x9876, 0x5432,
            0x00ff, 0x0104, 0x323c, 0x8000, // MOVE.W #$8000,D1
            0x60fe, 0x4e71,
        ];
        for (i, word) in words.iter().enumerate() {
            rom[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        let mut m = Machine::new(rom).unwrap();
        m.run(2).unwrap();
        assert_eq!(m.cpu.d[0], u32::MAX);
        assert_eq!(m.cpu.sr & 0xf, 8);
        m.step().unwrap();
        assert_eq!(m.cpu.d[0], 0xffff0000);
        assert_eq!(m.cpu.sr & 0xf, 4);
        m.step().unwrap();
        assert_eq!(m.cpu.d[1], 0x12345678);
        assert_eq!(m.cpu.sr & 0xf, 0);
        m.run(4).unwrap();
        assert_eq!(m.bus.read8(0xff0100), 0xab);
        assert_eq!(m.bus.read16(0xff0102), 0xcdef);
        assert_eq!(m.bus.read32(0xff0104), 0x98765432);
        assert_eq!(m.cpu.d[1], 0x12348000);
        assert_eq!(m.cpu.sr & 0xf, 8);
        let pc = m.cpu.pc;
        m.step().unwrap();
        assert_eq!(m.cpu.pc, pc);
    }

    #[test]
    fn retained_jsr_bsr_rts_and_jmp() {
        let mut rom = demo_rom();
        for (addr, word) in [
            (0x200, 0x4eb9u16),
            (0x202, 0),
            (0x204, 0x220),
            (0x206, 0x6118), // BSR.s $220
            (0x208, 0x4ef9),
            (0x20a, 0),
            (0x20c, 0x230),
            (0x220, 0x4e75),
            (0x230, 0x60fe),
        ] {
            rom[addr..addr + 2].copy_from_slice(&word.to_be_bytes());
        }
        let mut m = Machine::new(rom).unwrap();
        for return_pc in [0x206, 0x208] {
            m.step().unwrap();
            assert_eq!(m.cpu.pc, 0x220);
            assert_eq!(m.cpu.a[7], 0xfffefc);
            assert_eq!(m.bus.read32(0xfffefc), return_pc);
            m.step().unwrap();
            assert_eq!(m.cpu.pc, return_pc);
            assert_eq!(m.cpu.a[7], 0xffff00);
        }
        m.step().unwrap();
        assert_eq!(m.cpu.pc, 0x230);
    }
}
