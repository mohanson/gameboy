fn exec(sh: &str) -> std::io::Result<std::process::ExitStatus> {
    rog::println!("$ {}", sh);
    let mut parts = sh.split_whitespace();
    let prog = parts.next().unwrap();
    let args = parts;
    std::process::Command::new(prog).args(args).status()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !std::path::Path::new("./res/gb-test-roms").exists() {
        exec("git clone --depth=1 https://github.com/retrio/gb-test-roms ./res/gb-test-roms")?;
    }
    let mut case = std::collections::BTreeMap::new();
    case.insert("res/gb-test-roms/cgb_sound/cgb_sound.gb", 0);
    case.insert("res/gb-test-roms/cpu_instrs/cpu_instrs.gb", 1);
    case.insert("res/gb-test-roms/dmg_sound/dmg_sound.gb", 0);
    case.insert("res/gb-test-roms/instr_timing/instr_timing.gb", 1);
    case.insert("res/gb-test-roms/halt_bug.gb", 3);
    case.insert("res/gb-test-roms/interrupt_time/interrupt_time.gb", 3);
    case.insert("res/gb-test-roms/mem_timing/mem_timing.gb", 0);
    case.insert("res/gb-test-roms/mem_timing-2/mem_timing.gb", 0);
    case.insert("res/gb-test-roms/oam_bug/oam_bug.gb", 0);
    for (k, v) in case {
        match v {
            0 => {}
            1 => {
                exec(&format!("cargo run --release -- --mode blargg-serial-output -s 8 {}", k)).unwrap();
            }
            2 => {
                exec(&format!("cargo run --release -- --mode blargg-memory-output -s 8 {}", k)).unwrap();
            }
            3 => {
                if std::env::consts::OS == "windows" {
                    let _ = exec(&format!("cargo run --release -- --mode minifb -s 8 {}", k));
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}
