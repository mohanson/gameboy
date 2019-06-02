// Note: Game BoyTM, Game Boy PocketTM, Super Game BoyTM and Game Boy ColorTM are registered trademarks of
// Nintendo CO., LTD. © 1989 to 1999 by Nintendo CO., LTD.
use gameboy::gpu::{SCREEN_H, SCREEN_W};
use gameboy::motherboard::MotherBoard;

fn main() {
    rog::reg("gameboy::cartridge");

    let mut rom = String::from("");
    let mut c_audio = false;
    let mut c_scale = 2;
    {
        let mut ap = argparse::ArgumentParser::new();
        ap.set_description("Gameboy emulator");
        ap.refer(&mut c_audio)
            .add_option(&["-a"], argparse::StoreTrue, "Enable audio");
        ap.refer(&mut c_scale)
            .add_option(&["-x"], argparse::Store, "Scale the video");
        ap.refer(&mut rom).add_argument("rom", argparse::Store, "Rom name");
        ap.parse_args_or_exit();
    }

    let mut mother_board = MotherBoard::power_up(rom);
    let rom_name = mother_board.mmu.borrow().cartridge.title();

    let mut option = minifb::WindowOptions::default();
    option.resize = true;
    option.scale = match c_scale {
        1 => minifb::Scale::X1,
        2 => minifb::Scale::X2,
        4 => minifb::Scale::X4,
        8 => minifb::Scale::X8,
        _ => panic!("Supported scale: 1, 2, 4 or 8"),
    };
    let mut window =
        minifb::Window::new(format!("Gameboy - {}", rom_name).as_str(), SCREEN_W, SCREEN_H, option).unwrap();

    let mut window_buffer = vec![0x00; SCREEN_W * SCREEN_H];
    window.update_with_buffer(window_buffer.as_slice()).unwrap();

    loop {
        if !window.is_open() {
            break;
        }

        mother_board.next();
        if mother_board.cpu.flip() {
            if window.is_key_down(minifb::Key::Escape) {
                break;
            }
            if mother_board.check_and_reset_gpu_updated() {
                let mut i: usize = 0;
                for l in mother_board.mmu.borrow().gpu.data.iter() {
                    for w in l.iter() {
                        let b = u32::from(w[0]) << 16;
                        let g = u32::from(w[1]) << 8;
                        let r = u32::from(w[2]);
                        let a = 0xff00_0000;

                        window_buffer[i] = a | b | g | r;
                        i += 1;
                    }
                }
                window.update_with_buffer(window_buffer.as_slice()).unwrap();
            }

            let keys = vec![
                (minifb::Key::Right, gameboy::joypad::JoypadKey::Right),
                (minifb::Key::Up, gameboy::joypad::JoypadKey::Up),
                (minifb::Key::Left, gameboy::joypad::JoypadKey::Left),
                (minifb::Key::Down, gameboy::joypad::JoypadKey::Down),
                (minifb::Key::Z, gameboy::joypad::JoypadKey::A),
                (minifb::Key::X, gameboy::joypad::JoypadKey::B),
                (minifb::Key::Space, gameboy::joypad::JoypadKey::Select),
                (minifb::Key::Enter, gameboy::joypad::JoypadKey::Start),
            ];
            for (rk, vk) in &keys {
                if window.is_key_down(*rk) {
                    mother_board.keydown(vk.clone());
                } else {
                    mother_board.keyup(vk.clone());
                }
            }
        }
    }
}
