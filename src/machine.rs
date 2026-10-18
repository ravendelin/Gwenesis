use crate::{bus::Bus, cpu::Cpu};

pub(crate) struct Machine {
    pub(crate) cpu: Cpu,
    pub(crate) bus: Bus,
}
impl Machine {
    pub(crate) fn new(rom: Vec<u8>) -> Result<Self, String> {
        let mut bus = Bus::new(rom)?;
        let cpu = Cpu::reset(&mut bus)?;
        Ok(Self { cpu, bus })
    }
    pub(crate) fn step(&mut self) -> Result<(), String> {
        self.cpu.step(&mut self.bus)
    }
    pub(crate) fn run(&mut self, max_steps: u64) -> Result<(), String> {
        for _ in 0..max_steps {
            self.step()?;
        }
        Ok(())
    }
    pub(crate) fn frame(&self) -> Vec<u32> {
        self.bus.vdp.frame()
    }
}
