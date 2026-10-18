use crate::vdp::Vdp;

pub(crate) struct Bus {
    rom: Vec<u8>,
    ram: [u8; 65536],
    pub(crate) vdp: Vdp,
}
impl Bus {
    pub(crate) fn new(rom: Vec<u8>) -> Result<Self, String> {
        if rom.len() < 8 {
            return Err("ROM must contain initial SSP and PC vectors".into());
        }
        if rom.len() > 4 * 1024 * 1024 {
            return Err("ROM larger than 4 MiB unsupported".into());
        }
        Ok(Self {
            rom,
            ram: [0; 65536],
            vdp: Vdp::default(),
        })
    }
    pub(crate) fn contains_rom_address(&self, addr: u32) -> bool {
        (addr as usize) < self.rom.len()
    }
    pub(crate) fn read8(&self, addr: u32) -> u8 {
        let a = addr & 0xffffff;
        match a {
            0x000000..=0x3fffff => *self.rom.get(a as usize).unwrap_or(&0xff),
            0xff0000..=0xffffff => self.ram[(a & 0xffff) as usize],
            0xa10001 => 0x80,
            0xa10003 => 0x7f, // controller 1: all buttons released
            _ => 0xff,
        }
    }
    pub(crate) fn read16(&mut self, addr: u32) -> u16 {
        if addr & 1 != 0 {
            return 0xffff;
        }
        if (0xc00000..=0xc00003).contains(&(addr & 0xffffff)) {
            return self.vdp.data_read();
        }
        if (0xc00004..=0xc00007).contains(&(addr & 0xffffff)) {
            return 0x3400;
        }
        u16::from_be_bytes([self.read8(addr), self.read8(addr.wrapping_add(1))])
    }
    pub(crate) fn read32(&mut self, addr: u32) -> u32 {
        ((self.read16(addr) as u32) << 16) | self.read16(addr.wrapping_add(2)) as u32
    }
    pub(crate) fn write8(&mut self, addr: u32, value: u8) {
        if addr & 0xff0000 == 0xff0000 {
            self.ram[(addr & 0xffff) as usize] = value;
        }
    }
    pub(crate) fn write16(&mut self, addr: u32, value: u16) {
        let a = addr & 0xffffff;
        match a {
            0xc00000..=0xc00003 => self.vdp.data_write(value),
            0xc00004..=0xc00007 => self.vdp.control(value),
            0xff0000..=0xfffffe => {
                self.write8(a, (value >> 8) as u8);
                self.write8(a + 1, value as u8);
            }
            _ => {}
        }
    }
    pub(crate) fn write32(&mut self, addr: u32, value: u32) {
        self.write16(addr, (value >> 16) as u16);
        self.write16(addr.wrapping_add(2), value as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ram_roundtrip_big_endian() {
        let mut bus = Bus::new(vec![0; 8]).unwrap();
        bus.write32(0xff0100, 0x12345678);
        assert_eq!(bus.read32(0xff0100), 0x12345678);
        assert_eq!(bus.read16(0xff0100), 0x1234);
        assert_eq!(bus.read8(0xff0102), 0x56);
        assert_eq!(bus.read8(0xff0103), 0x78);
        bus.write16(0xfffffe, 0xabcd);
        assert_eq!(bus.read16(0xfffffe), 0xabcd);
        bus.write8(0x01ff0000, 0xef);
        assert_eq!(bus.read8(0xff0000), 0xef);
    }
    #[test]
    fn rom_size_validation() {
        assert!(Bus::new(vec![0; 7]).is_err());
        assert!(Bus::new(vec![0; 4 * 1024 * 1024 + 1]).is_err());
    }
}
