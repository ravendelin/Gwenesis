mod bus;
mod cpu;
mod demo;
mod machine;
mod vdp;

use crate::{
    demo::demo_rom,
    machine::Machine,
    vdp::{H, W},
};
use std::{env, fs, io::Write, path::Path};

fn ppm(path: &Path, pixels: &[u32]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    write!(f, "P6\n{W} {H}\n255\n")?;
    for &v in pixels {
        f.write_all(&[(v >> 16) as u8, (v >> 8) as u8, v as u8])?;
    }
    Ok(())
}
#[cfg(feature = "window")]
fn show(mut machine: Machine) -> Result<(), String> {
    use minifb::{Key, Window, WindowOptions};
    let mut window = Window::new(
        "Gwenesis",
        W,
        H,
        WindowOptions {
            scale: minifb::Scale::X2,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut cpu_error = None;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        if cpu_error.is_none() {
            if let Err(e) = machine.run(2_000) {
                eprintln!("CPU halted: {e}");
                cpu_error = Some(e);
            }
        }
        window
            .update_with_buffer(&machine.frame(), W, H)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: gwenesis [rom.bin | --demo] [--steps N] [--ppm frame.ppm] [--window]\nDefault: --demo --steps 2000 --ppm frame.ppm\n--window requires cargo run --features window -- --demo --window");
        return Ok(());
    }
    let mut rom_file: Option<String> = None;
    let mut steps = 2000u64;
    let mut output = "frame.ppm".to_string();
    let mut window = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {}
            "--window" => window = true,
            "--steps" | "--ppm" => {
                if i + 1 == args.len() {
                    return Err(format!("{} requires a value", args[i]).into());
                }
                if args[i] == "--steps" {
                    steps = args[i + 1].parse()?;
                } else {
                    output = args[i + 1].clone();
                }
                i += 1;
            }
            s if s.starts_with('-') => return Err(format!("Unknown option: {s}").into()),
            path => rom_file = Some(path.to_string()),
        }
        i += 1;
    }
    let rom = if let Some(path) = rom_file {
        fs::read(path)?
    } else {
        demo_rom()
    };
    let mut machine = Machine::new(rom)?;
    if window {
        #[cfg(feature = "window")]
        {
            return show(machine).map_err(Into::into);
        }
        #[cfg(not(feature = "window"))]
        {
            return Err("Rebuild with --features window".into());
        }
    }
    machine.run(steps)?;
    ppm(Path::new(&output), &machine.frame())?;
    println!(
        "Executed {} instructions; PC=${:06X}; frame written to {output}",
        machine.cpu.steps, machine.cpu.pc
    );
    Ok(())
}
