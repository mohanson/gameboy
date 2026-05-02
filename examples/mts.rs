fn exec(sh: &str) {
    rog::println!("$ {}", sh);
    let mut parts = sh.split_whitespace();
    let prog = parts.next().unwrap();
    let args = parts;
    assert_eq!(std::process::Command::new(prog).args(args).status().unwrap().code().unwrap(), 0);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !std::path::Path::new("res/mts").exists() {
        exec(
            "wget https://gekkio.fi/files/mooneye-test-suite/mts-20240926-1737-443f6e1/mts-20240926-1737-443f6e1.zip -O res/mts-20240926-1737-443f6e1.zip",
        );
        exec("unzip res/mts-20240926-1737-443f6e1.zip -d res");
        exec("rm res/mts-20240926-1737-443f6e1.zip");
        exec("mv res/mts-20240926-1737-443f6e1 res/mts");
    }

    let mut case = std::collections::BTreeMap::new();
    // case.insert("res/mts/acceptance/add_sp_e_timing.gb", 0);
    case.insert("res/mts/acceptance/bits/mem_oam.gb", 1);
    case.insert("res/mts/acceptance/bits/reg_f.gb", 1);
    case.insert("res/mts/acceptance/bits/unused_hwio-GS.gb", 1);
    case.insert("res/mts/acceptance/boot_div-S.gb", 9);
    case.insert("res/mts/acceptance/boot_div-dmg0.gb", 9);
    // case.insert("res/mts/acceptance/boot_div-dmgABCmgb.gb", 0);
    // case.insert("res/mts/acceptance/boot_div2-S.gb", 0);
    // case.insert("res/mts/acceptance/boot_hwio-S.gb", 0);
    // case.insert("res/mts/acceptance/boot_hwio-dmg0.gb", 0);
    // case.insert("res/mts/acceptance/boot_hwio-dmgABCmgb.gb", 0);
    // case.insert("res/mts/acceptance/boot_regs-dmg0.gb", 0);
    // case.insert("res/mts/acceptance/boot_regs-dmgABC.gb", 0);
    // case.insert("res/mts/acceptance/boot_regs-mgb.gb", 0);
    // case.insert("res/mts/acceptance/boot_regs-sgb.gb", 0);
    // case.insert("res/mts/acceptance/boot_regs-sgb2.gb", 0);
    // case.insert("res/mts/acceptance/call_cc_timing.gb", 0);
    // case.insert("res/mts/acceptance/call_cc_timing2.gb", 0);
    // case.insert("res/mts/acceptance/call_timing.gb", 0);
    // case.insert("res/mts/acceptance/call_timing2.gb", 0);
    // case.insert("res/mts/acceptance/di_timing-GS.gb", 0);
    case.insert("res/mts/acceptance/div_timing.gb", 1);
    case.insert("res/mts/acceptance/ei_sequence.gb", 1);
    case.insert("res/mts/acceptance/ei_timing.gb", 1);
    case.insert("res/mts/acceptance/halt_ime0_ei.gb", 1);
    case.insert("res/mts/acceptance/halt_ime0_nointr_timing.gb", 1);
    case.insert("res/mts/acceptance/halt_ime1_timing.gb", 1);
    case.insert("res/mts/acceptance/halt_ime1_timing2-GS.gb", 1);
    case.insert("res/mts/acceptance/if_ie_registers.gb", 1);
    case.insert("res/mts/acceptance/instr/daa.gb", 1);
    // case.insert("res/mts/acceptance/interrupts/ie_push.gb", 0);
    case.insert("res/mts/acceptance/intr_timing.gb", 1);
    // case.insert("res/mts/acceptance/jp_cc_timing.gb", 0);
    // case.insert("res/mts/acceptance/jp_timing.gb", 0);
    // case.insert("res/mts/acceptance/ld_hl_sp_e_timing.gb", 0);
    case.insert("res/mts/acceptance/oam_dma/basic.gb", 1);
    // case.insert("res/mts/acceptance/oam_dma/reg_read.gb", 0);
    // case.insert("res/mts/acceptance/oam_dma/sources-GS.gb", 0);
    // case.insert("res/mts/acceptance/oam_dma_restart.gb", 0);
    // case.insert("res/mts/acceptance/oam_dma_start.gb", 0);
    // case.insert("res/mts/acceptance/oam_dma_timing.gb", 0);
    // case.insert("res/mts/acceptance/pop_timing.gb", 0);
    // case.insert("res/mts/acceptance/ppu/hblank_ly_scx_timing-GS.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_1_2_timing-GS.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_2_0_timing.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_2_mode0_timing.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_2_mode0_timing_sprites.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_2_mode3_timing.gb", 0);
    // case.insert("res/mts/acceptance/ppu/intr_2_oam_ok_timing.gb", 0);
    // case.insert("res/mts/acceptance/ppu/lcdon_timing-GS.gb", 0);
    // case.insert("res/mts/acceptance/ppu/lcdon_write_timing-GS.gb", 0);
    // case.insert("res/mts/acceptance/ppu/stat_irq_blocking.gb", 0);
    // case.insert("res/mts/acceptance/ppu/stat_lyc_onoff.gb", 0);
    // case.insert("res/mts/acceptance/ppu/vblank_stat_intr-GS.gb", 0);
    // case.insert("res/mts/acceptance/push_timing.gb", 0);
    case.insert("res/mts/acceptance/rapid_di_ei.gb", 1);
    // case.insert("res/mts/acceptance/ret_cc_timing.gb", 0);
    // case.insert("res/mts/acceptance/ret_timing.gb", 0);
    case.insert("res/mts/acceptance/reti_intr_timing.gb", 1);
    // case.insert("res/mts/acceptance/reti_timing.gb", 0);
    // case.insert("res/mts/acceptance/rst_timing.gb", 0);
    // case.insert("res/mts/acceptance/serial/boot_sclk_align-dmgABCmgb.gb", 0);
    case.insert("res/mts/acceptance/timer/div_write.gb", 1);
    case.insert("res/mts/acceptance/timer/rapid_toggle.gb", 1);
    case.insert("res/mts/acceptance/timer/tim00.gb", 1);
    case.insert("res/mts/acceptance/timer/tim00_div_trigger.gb", 1);
    case.insert("res/mts/acceptance/timer/tim01.gb", 1);
    case.insert("res/mts/acceptance/timer/tim01_div_trigger.gb", 1);
    case.insert("res/mts/acceptance/timer/tim10.gb", 1);
    case.insert("res/mts/acceptance/timer/tim10_div_trigger.gb", 1);
    case.insert("res/mts/acceptance/timer/tim11.gb", 1);
    case.insert("res/mts/acceptance/timer/tim11_div_trigger.gb", 1);
    case.insert("res/mts/acceptance/timer/tima_reload.gb", 1);
    case.insert("res/mts/acceptance/timer/tima_write_reloading.gb", 1);
    case.insert("res/mts/acceptance/timer/tma_write_reloading.gb", 1);
    // case.insert("res/mts/emulator-only/mbc1/bits_bank1.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/bits_bank2.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/bits_mode.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/bits_ramg.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/multicart_rom_8Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/ram_256kb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/ram_64kb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_16Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_1Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_2Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_4Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_512kb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc1/rom_8Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/bits_ramg.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/bits_romb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/bits_unused.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/ram.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/rom_1Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/rom_2Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc2/rom_512kb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_16Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_1Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_2Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_32Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_4Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_512kb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_64Mb.gb", 0);
    // case.insert("res/mts/emulator-only/mbc5/rom_8Mb.gb", 0);
    // case.insert("res/mts/madness/mgb_oam_dma_halt_sprites.gb", 0);
    // case.insert("res/mts/manual-only/sprite_priority.gb", 0);
    // case.insert("res/mts/misc/bits/unused_hwio-C.gb", 0);
    // case.insert("res/mts/misc/boot_div-A.gb", 0);
    // case.insert("res/mts/misc/boot_div-cgb0.gb", 0);
    // case.insert("res/mts/misc/boot_div-cgbABCDE.gb", 0);
    // case.insert("res/mts/misc/boot_hwio-C.gb", 0);
    // case.insert("res/mts/misc/boot_regs-A.gb", 0);
    // case.insert("res/mts/misc/boot_regs-cgb.gb", 0);
    // case.insert("res/mts/misc/ppu/vblank_stat_intr-C.gb", 0);
    // case.insert("res/mts/utils/bootrom_dumper.gb", 0);
    // case.insert("res/mts/utils/dump_boot_hwio.gb", 0);

    for (k, v) in case {
        match v {
            0 => {}
            1 => {
                exec(&format!("cargo run --release -- --mode mts -s 8 {}", k));
            }
            9 => {}
            _ => unreachable!(),
        }
    }

    Ok(())
}
