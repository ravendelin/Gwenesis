use std::{env, fs, io::Write, path::Path};

const W: usize = 320;
const H: usize = 224;

#[derive(Clone)]
struct Vdp {
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
        Self { vram: [0; 65536], cram: [0; 64], vsram: [0; 40], reg: [0; 24], addr: 0, code: 0, first_command: None }
    }
}
impl Vdp {
    fn control(&mut self, word: u16) {
        if let Some(first) = self.first_command.take() {
            self.addr = ((first & 0x3fff) | ((word & 3) << 14)) as u16;
            self.code = ((first >> 14) as u8) | (((word >> 2) & 0x3c) as u8);
        } else if word & 0xe000 == 0x8000 {
            let index = ((word >> 8) & 0x1f) as usize;
            if index < 24 { self.reg[index] = word as u8; }
        } else {
            self.first_command = Some(word);
        }
    }
    fn data_write(&mut self, value: u16) {
        let addr = self.addr as usize;
        match self.code & 0x0f {
            1 => { self.vram[addr] = (value >> 8) as u8; self.vram[(addr + 1) & 0xffff] = value as u8; }
            3 => self.cram[(addr >> 1) & 63] = value,
            5 => self.vsram[(addr >> 1) % 40] = value,
            _ => {}
        }
        self.addr = self.addr.wrapping_add(self.reg[15] as u16);
    }
    fn rgb(color: u16) -> u32 {
        let r = ((color >> 1) & 7) as u32 * 36;
        let g = ((color >> 5) & 7) as u32 * 36;
        let b = ((color >> 9) & 7) as u32 * 36;
        (r << 16) | (g << 8) | b
    }
    fn frame(&self) -> Vec<u32> {
        let backdrop = (self.reg[7] & 0x3f) as usize;
        let mut out = vec![Self::rgb(self.cram[backdrop]); W * H];
        // Simplified plane A: 64x32 tiles, no scrolling/sprites/priority/window.
        if self.reg[1] & 0x40 == 0 { return out; }
        let base = ((self.reg[2] as usize & 0x38) << 10) & 0xffff;
        for y in 0..H {
            for x in 0..W {
                let map_idx = (base + (((y / 8) * 64 + x / 8) * 2)) & 0xffff;
                let entry = u16::from_be_bytes([self.vram[map_idx], self.vram[(map_idx + 1) & 0xffff]]);
                let tile = (entry & 0x7ff) as usize;
                let xx = if entry & 0x800 != 0 { 7 - x % 8 } else { x % 8 };
                let yy = if entry & 0x1000 != 0 { 7 - y % 8 } else { y % 8 };
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

struct Machine {
    rom: Vec<u8>,
    ram: [u8; 65536],
    d: [u32; 8],
    a: [u32; 8],
    pc: u32,
    sr: u16,
    vdp: Vdp,
    steps: u64,
}
impl Machine {
    fn new(rom: Vec<u8>) -> Result<Self, String> {
        if rom.len() < 8 { return Err("ROM must contain initial SSP and PC vectors".into()); }
        if rom.len() > 4 * 1024 * 1024 { return Err("ROM larger than 4 MiB unsupported".into()); }
        let mut machine = Self { rom, ram: [0; 65536], d: [0; 8], a: [0; 8], pc: 0, sr: 0x2700, vdp: Vdp::default(), steps: 0 };
        machine.a[7] = machine.read32(0);
        machine.pc = machine.read32(4);
        if machine.pc as usize >= machine.rom.len() { return Err(format!("Invalid reset PC ${:06X}", machine.pc)); }
        Ok(machine)
    }
    fn read8(&self, addr: u32) -> u8 {
        let a = addr & 0xffffff;
        match a {
            0x000000..=0x3fffff => *self.rom.get(a as usize).unwrap_or(&0xff),
            0xff0000..=0xffffff => self.ram[(a & 0xffff) as usize],
            0xa10001 => 0x80,
            0xa10003 => 0x7f, // controller 1: all buttons released
            _ => 0xff,
        }
    }
    fn read16(&self, addr: u32) -> u16 {
        if addr & 1 != 0 { return 0xffff; }
        if (0xc00004..=0xc00007).contains(&(addr & 0xffffff)) { return 0x3400; }
        u16::from_be_bytes([self.read8(addr), self.read8(addr.wrapping_add(1))])
    }
    fn read32(&self, addr: u32) -> u32 { ((self.read16(addr) as u32) << 16) | self.read16(addr.wrapping_add(2)) as u32 }
    fn write8(&mut self, addr: u32, value: u8) {
        if addr & 0xff0000 == 0xff0000 { self.ram[(addr & 0xffff) as usize] = value; }
    }
    fn write16(&mut self, addr: u32, value: u16) {
        let a = addr & 0xffffff;
        match a {
            0xc00000..=0xc00003 => self.vdp.data_write(value),
            0xc00004..=0xc00007 => self.vdp.control(value),
            0xff0000..=0xfffffe => { self.write8(a, (value >> 8) as u8); self.write8(a + 1, value as u8); }
            _ => {}
        }
    }
    fn write32(&mut self, addr: u32, value: u32) {
        self.write16(addr, (value >> 16) as u16);
        self.write16(addr.wrapping_add(2), value as u16);
    }
    fn fetch16(&mut self) -> u16 { let value = self.read16(self.pc); self.pc = self.pc.wrapping_add(2); value }
    fn fetch32(&mut self) -> u32 { let value = self.fetch16() as u32; (value << 16) | self.fetch16() as u32 }
    fn set_nz(&mut self, val: u32, bits: u32) {
        self.sr &= !0x0f;
        if val & ((1u32 << (bits - 1)) as u32) != 0 { self.sr |= 8; }
        if val & (if bits == 32 { u32::MAX } else { (1 << bits) - 1 }) == 0 { self.sr |= 4; }
    }
    fn step(&mut self) -> Result<(), String> {
        let at = self.pc;
        let op = self.fetch16();
        match op {
            0x4e71 => {}, // NOP
            0x4e75 => { self.pc = self.read32(self.a[7]); self.a[7] = self.a[7].wrapping_add(4); }, // RTS
            0x4ef9 => { self.pc = self.fetch32(); }, // JMP abs.l
            0x4eb9 => { let dest = self.fetch32(); self.a[7] = self.a[7].wrapping_sub(4); self.write32(self.a[7], self.pc); self.pc = dest; }, // JSR
            0x33fc => { let val = self.fetch16(); let addr = self.fetch32(); self.write16(addr, val); }, // MOVE.W #imm,abs.l
            0x23fc => { let val = self.fetch32(); let addr = self.fetch32(); self.write32(addr, val); }, // MOVE.L #imm,abs.l
            0x13fc => { let val = self.fetch16() as u8; let addr = self.fetch32(); self.write8(addr, val); },
            0x303c..=0x3e3c if (op & 0x0e00) <= 0x0e00 => { let reg = ((op >> 9) & 7) as usize; let val = self.fetch16(); self.d[reg] = (self.d[reg] & 0xffff0000) | val as u32; self.set_nz(val as u32, 16); },
            0x203c..=0x2e3c if op & 0x01ff == 0x003c => { let reg = ((op >> 9) & 7) as usize; let val = self.fetch32(); self.d[reg] = val; self.set_nz(val, 32); },
            0x7000..=0x7fff => { let reg = ((op >> 9) & 7) as usize; let val = (op as u8 as i8 as i32) as u32; self.d[reg] = val; self.set_nz(val, 32); }, // MOVEQ
            0x6000..=0x60ff => { let disp = if op & 0xff == 0 { self.fetch16() as i16 as i32 } else { (op as u8 as i8) as i32 }; self.pc = self.pc.wrapping_add_signed(disp); }, // BRA
            0x6100..=0x61ff => { let disp = if op & 0xff == 0 { self.fetch16() as i16 as i32 } else { (op as u8 as i8) as i32 }; self.a[7] = self.a[7].wrapping_sub(4); self.write32(self.a[7], self.pc); self.pc = self.pc.wrapping_add_signed(disp); }, // BSR
            _ => return Err(format!("Unsupported 68000 opcode ${op:04X} at ${at:06X}")),
        }
        self.steps += 1;
        Ok(())
    }
    fn run(&mut self, max_steps: u64) -> Result<(), String> { for _ in 0..max_steps { self.step()?; } Ok(()) }
}
fn put_word(rom: &mut [u8], addr: usize, val: u16) { rom[addr..addr + 2].copy_from_slice(&val.to_be_bytes()); }
fn put_long(rom: &mut [u8], addr: usize, val: u32) { rom[addr..addr + 4].copy_from_slice(&val.to_be_bytes()); }
fn demo_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x400];
    put_long(&mut rom, 0, 0xff_ff_00); put_long(&mut rom, 4, 0x200);
    rom[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    let mut pc = 0x200;
    let mut emit = |value: u16| { put_word(&mut rom, pc, value); pc += 2; };
    // Display ON, backdrop palette index 1, auto-increment 2.
    for regval in [0x8144, 0x8701, 0x8f02] { emit(0x33fc); emit(regval); emit(0x00c0); emit(0x0004); }
    // CRAM write to address 2: first control word $C002, second $0000.
    for val in [0xc002, 0x0000] { emit(0x33fc); emit(val); emit(0x00c0); emit(0x0004); }
    emit(0x33fc); emit(0x000e); emit(0x00c0); emit(0x0000); // bright red
    emit(0x60fe); // BRA -2 forever
    rom
}
fn ppm(path: &Path, pixels: &[u32]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    write!(f, "P6\n{W} {H}\n255\n")?;
    for &v in pixels { f.write_all(&[(v >> 16) as u8, (v >> 8) as u8, v as u8])?; }
    Ok(())
}
#[cfg(feature = "window")]
fn show(mut machine: Machine) -> Result<(), String> {
    use minifb::{Key, Window, WindowOptions};
    let mut window = Window::new("Genesis RS | educational core", W, H, WindowOptions { scale: minifb::Scale::X2, ..WindowOptions::default() }).map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut cpu_error = None;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        if cpu_error.is_none() { if let Err(e) = machine.run(2_000) { eprintln!("CPU halted: {e}"); cpu_error = Some(e); } }
        window.update_with_buffer(&machine.vdp.frame(), W, H).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: genesis-rs [rom.bin | --demo] [--steps N] [--ppm frame.ppm] [--window]\nDefault: --demo --steps 2000 --ppm frame.ppm\n--window requires cargo run --features window -- --demo --window");
        return Ok(());
    }
    let mut rom_file: Option<String> = None;
    let mut steps = 2000u64;
    let mut output = "frame.ppm".to_string();
    let mut window = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {},
            "--window" => window = true,
            "--steps" | "--ppm" => {
                if i + 1 == args.len() { return Err(format!("{} requires a value", args[i]).into()); }
                if args[i] == "--steps" { steps = args[i + 1].parse()?; } else { output = args[i + 1].clone(); }
                i += 1;
            }
            s if s.starts_with('-') => return Err(format!("Unknown option: {s}").into()),
            path => rom_file = Some(path.to_string()),
        }
        i += 1;
    }
    let rom = if let Some(path) = rom_file { fs::read(path)? } else { demo_rom() };
    let mut machine = Machine::new(rom)?;
    if window {
        #[cfg(feature = "window")]
        { return show(machine).map_err(Into::into); }
        #[cfg(not(feature = "window"))]
        { return Err("Rebuild with --features window".into()); }
    }
    machine.run(steps)?;
    ppm(Path::new(&output), &machine.vdp.frame())?;
    println!("Executed {} instructions; PC=${:06X}; frame written to {output}", machine.steps, machine.pc);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn demo_runs_and_sets_backdrop() {
        let mut m = Machine::new(demo_rom()).unwrap();
        m.run(100).unwrap();
        assert_eq!(m.vdp.reg[7], 1);
        assert_eq!(m.vdp.cram[1], 0x000e);
        assert_eq!(m.vdp.frame()[0], 0xfc0000);
    }
    #[test] fn ram_roundtrip_big_endian() {
        let mut m = Machine::new(demo_rom()).unwrap();
        m.write32(0xff0100, 0x12345678);
        assert_eq!(m.read32(0xff0100), 0x12345678);
    }
    #[test] fn unknown_opcode_fails_clearly() {
        let mut r = demo_rom(); put_word(&mut r, 0x200, 0xffff);
        assert!(Machine::new(r).unwrap().step().unwrap_err().contains("FFFF"));
    }
}
