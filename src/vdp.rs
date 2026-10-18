pub(crate) const W: usize = 320;
pub(crate) const H: usize = 224;

#[derive(Clone)]
pub(crate) struct Vdp {
    vram: [u8; 65536],
    cram: [u16; 64],
    vsram: [u16; 40],
    reg: [u8; 24],
    addr: u16,
    code: u8,
    first_command: Option<u16>,
}
impl Default for Vdp {
    fn default() -> Self {
        Self {
            vram: [0; 65536],
            cram: [0; 64],
            vsram: [0; 40],
            reg: [0; 24],
            addr: 0,
            code: 0,
            first_command: None,
        }
    }
}
impl Vdp {
    pub(crate) fn control(&mut self, word: u16) {
        if let Some(first) = self.first_command.take() {
            self.addr = (first & 0x3fff) | ((word & 3) << 14);
            self.code = ((first >> 14) as u8) | (((word >> 2) & 0x3c) as u8);
        } else if word & 0xe000 == 0x8000 {
            let index = ((word >> 8) & 0x1f) as usize;
            if index < 24 {
                self.reg[index] = word as u8;
            }
        } else {
            self.first_command = Some(word);
        }
    }
    pub(crate) fn data_write(&mut self, value: u16) {
        self.first_command = None;
        let addr = self.addr as usize;
        match self.code & 0x0f {
            1 => {
                // Mode 5, 64 KB: bit 0 swaps the byte lanes of one word.
                // XOR keeps odd-address writes inside the aligned word.
                self.vram[addr & 0xffff] = (value >> 8) as u8;
                self.vram[(addr ^ 1) & 0xffff] = value as u8;
            }
            3 => self.cram[(addr >> 1) & 63] = value,
            5 => self.vsram[(addr >> 1) % 40] = value,
            _ => {}
        }
        self.addr = self.addr.wrapping_add(self.reg[15] as u16);
    }
    pub(crate) fn data_read(&mut self) -> u16 {
        self.first_command = None;
        let addr = self.addr as usize;
        let value = match self.code & 0x0f {
            0 => {
                // Word reads ignore bit 0, without changing the address counter.
                let aligned = addr & 0xfffe;
                u16::from_be_bytes([self.vram[aligned], self.vram[aligned | 1]])
            }
            8 => self.cram[(addr >> 1) & 63],
            4 => self.vsram[(addr >> 1) % 40],
            _ => 0xffff,
        };
        self.addr = self.addr.wrapping_add(self.reg[15] as u16);
        value
    }

    fn rgb(color: u16) -> u32 {
        let r = ((color >> 1) & 7) as u32 * 36;
        let g = ((color >> 5) & 7) as u32 * 36;
        let b = ((color >> 9) & 7) as u32 * 36;
        (r << 16) | (g << 8) | b
    }
    pub(crate) fn frame(&self) -> Vec<u32> {
        let backdrop = (self.reg[7] & 0x3f) as usize;
        let mut out = vec![Self::rgb(self.cram[backdrop]); W * H];
        // Simplified plane A: 64x32 tiles, no scrolling/sprites/priority/window.
        if self.reg[1] & 0x40 == 0 {
            return out;
        }
        let base = ((self.reg[2] as usize & 0x38) << 10) & 0xffff;
        for y in 0..H {
            for x in 0..W {
                let map_idx = (base + (((y / 8) * 64 + x / 8) * 2)) & 0xffff;
                let entry =
                    u16::from_be_bytes([self.vram[map_idx], self.vram[(map_idx + 1) & 0xffff]]);
                let tile = (entry & 0x7ff) as usize;
                let xx = if entry & 0x800 != 0 { 7 - x % 8 } else { x % 8 };
                let yy = if entry & 0x1000 != 0 {
                    7 - y % 8
                } else {
                    y % 8
                };
                let pixel_addr = (tile * 32 + yy * 4 + xx / 2) & 0xffff;
                let bits = self.vram[pixel_addr];
                let pal_index = if xx % 2 == 0 { bits >> 4 } else { bits & 15 };
                if pal_index != 0 {
                    let palette = ((entry >> 13) & 3) as usize;
                    out[y * W + x] = Self::rgb(self.cram[palette * 16 + pal_index as usize]);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(vdp: &mut Vdp, addr: u16, code: u8) {
        vdp.control((addr & 0x3fff) | (u16::from(code & 3) << 14));
        vdp.control((addr >> 14) | (u16::from(code & 0x3c) << 2));
    }

    #[test]
    fn control_decodes_address_and_code_bits() {
        let mut v = Vdp::default();
        // Literal commands independently verify the packing of both words.
        for (first, second, addr, code) in [
            (0x4000, 0x0003, 0xc000, 1),
            (0xc07e, 0x0000, 0x007e, 3),
            (0x007e, 0x0020, 0x007e, 8),
            (0x7fff, 0x0003, 0xffff, 1),
            (0x4000, 0x0080, 0, 0x21),
        ] {
            v.control(first);
            v.control(second);
            assert_eq!((v.addr, v.code), (addr, code));
            assert_eq!(v.first_command, None);
        }
        v.control(0x8f02);
        assert_eq!(v.reg[15], 2);
        assert_eq!(v.first_command, None);
    }

    #[test]
    fn vram_roundtrip_boundaries_and_increment() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0xfffe, 1);
        v.data_write(0x1234);
        v.data_write(0xabcd);
        assert_eq!(v.addr, 2);
        command(&mut v, 0xfffe, 0);
        assert_eq!(v.data_read(), 0x1234);
        assert_eq!(v.data_read(), 0xabcd);
        command(&mut v, 0xffff, 1);
        v.data_write(0x5678);
        command(&mut v, 0xffff, 0);
        assert_eq!(v.data_read(), 0x7856);
        v.control(0x8f04);
        command(&mut v, 0x100, 1);
        v.data_write(0x1122);
        v.data_write(0x3344);
        command(&mut v, 0x100, 0);
        assert_eq!(v.data_read(), 0x1122);
        assert_eq!(v.data_read(), 0x3344);
        v.control(0x8f00);
        command(&mut v, 0x100, 1);
        v.data_write(0x5566);
        v.data_write(0x7788);
        command(&mut v, 0x100, 0);
        assert_eq!(v.data_read(), 0x7788);
        assert_eq!(v.addr, 0x100);
    }

    #[test]
    fn vram_aligned_write_preserves_big_endian_layout() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0x0100, 1);
        v.data_write(0xa1b2);
        assert_eq!(v.vram[0x100], 0xa1);
        assert_eq!(v.vram[0x101], 0xb2);
        assert_eq!(v.addr, 0x0102);
    }

    #[test]
    fn vram_odd_write_swaps_bytes_within_aligned_word() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0x0102, 1);
        v.data_write(0x5a6b); // Sentinel in the following word.
        command(&mut v, 0x0101, 1);
        v.data_write(0xc3d4);
        assert_eq!(v.vram[0x100], 0xd4);
        assert_eq!(v.vram[0x101], 0xc3);
        assert_eq!(v.vram[0x102], 0x5a);
        assert_eq!(v.vram[0x103], 0x6b);
        assert_eq!(v.addr, 0x0103);
    }

    #[test]
    fn vram_word_read_ignores_address_bit_zero() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0x0101, 1);
        v.data_write(0xc3d4);
        command(&mut v, 0x0100, 0);
        assert_eq!(v.data_read(), 0xd4c3);
        assert_eq!(v.addr, 0x0102);
        command(&mut v, 0x0101, 0);
        assert_eq!(v.data_read(), 0xd4c3);
        assert_eq!(v.addr, 0x0103);
    }

    #[test]
    fn vram_boundary_access_stays_in_word_and_increment_wraps() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0, 1);
        v.data_write(0x5a6b);
        command(&mut v, 0xfffe, 1);
        v.data_write(0xa1b2);
        assert_eq!(v.vram[0xfffe], 0xa1);
        assert_eq!(v.vram[0xffff], 0xb2);
        assert_eq!(v.addr, 0);
        command(&mut v, 0xffff, 1);
        v.data_write(0xc3d4);
        assert_eq!(v.vram[0xfffe], 0xd4);
        assert_eq!(v.vram[0xffff], 0xc3);
        assert_eq!(v.vram[0], 0x5a);
        assert_eq!(v.vram[1], 0x6b);
        assert_eq!(v.addr, 1);
        command(&mut v, 0xfffe, 0);
        assert_eq!(v.data_read(), 0xd4c3);
        assert_eq!(v.addr, 0);
        assert_eq!(v.data_read(), 0x5a6b);
        command(&mut v, 0xffff, 0);
        assert_eq!(v.data_read(), 0xd4c3);
        assert_eq!(v.addr, 1);
        assert_eq!(v.data_read(), 0x5a6b);
        assert_eq!(v.addr, 3);
        v.control(0x8f01);
        command(&mut v, 0xffff, 1);
        v.data_write(0x1234);
        assert_eq!(v.addr, 0);
        command(&mut v, 0xffff, 0);
        assert_eq!(v.data_read(), 0x3412);
        assert_eq!(v.addr, 0);
    }

    #[test]
    fn cram_roundtrip_boundary_and_address_wrap() {
        let mut v = Vdp::default();
        v.control(0x8f02);
        command(&mut v, 0x7e, 3);
        v.data_write(0x0eee);
        v.data_write(0x000e);
        command(&mut v, 0x7e, 8);
        assert_eq!(v.data_read(), 0x0eee);
        assert_eq!(v.data_read(), 0x000e);
        command(&mut v, 0xfffe, 3);
        v.data_write(0x00e0);
        assert_eq!(v.addr, 0);
        command(&mut v, 0x7e, 8);
        assert_eq!(v.data_read(), 0x00e0);
    }
}
