// Note: Game BoyTM, Game Boy PocketTM, Super Game BoyTM and Game Boy ColorTM are registered trademarks of
// Nintendo CO., LTD. © 1989 to 1999 by Nintendo CO., LTD.
use gameboy::gpu::{SCREEN_H, SCREEN_W};
use gameboy::motherboard::MotherBoard;
use gameboy::sound::AudioPlayer;
use std::sync::{Arc, Mutex};

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

    if c_audio {
        let player = CpalPlayer::get();
        match player {
            Some(v) => mother_board.enable_audio(Box::new(v) as Box<AudioPlayer>),
            None => {
                panic!("Could not open audio device");
            }
        }
    }

    loop {
        // Stop the program, if the GUI is closed by the user
        if !window.is_open() {
            break;
        }

        // Execute an instruction
        mother_board.next();

        // Update the window
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

        if !mother_board.cpu.flip() {
            continue;
        }

        // Handling keyboard events
        if window.is_key_down(minifb::Key::Escape) {
            break;
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
                mother_board.mmu.borrow_mut().joypad.keydown(vk.clone());
            } else {
                mother_board.mmu.borrow_mut().joypad.keyup(vk.clone());
            }
        }
    }

    mother_board.mmu.borrow_mut().cartridge.sav();
}

struct CpalPlayer {
    buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    sample_rate: u32,
}

impl CpalPlayer {
    fn get() -> Option<CpalPlayer> {
        let device = match cpal::default_output_device() {
            Some(e) => e,
            None => return None,
        };

        let mut wanted_samplerate = None;
        let mut wanted_sampleformat = None;
        let supported_formats = match device.supported_output_formats() {
            Ok(e) => e,
            Err(_) => return None,
        };
        for f in supported_formats {
            match wanted_samplerate {
                None => wanted_samplerate = Some(f.max_sample_rate),
                Some(cpal::SampleRate(r)) if r < f.max_sample_rate.0 && r < 192_000 => {
                    wanted_samplerate = Some(f.max_sample_rate)
                }
                _ => {}
            }
            match wanted_sampleformat {
                None => wanted_sampleformat = Some(f.data_type),
                Some(cpal::SampleFormat::F32) => {}
                Some(_) if f.data_type == cpal::SampleFormat::F32 => wanted_sampleformat = Some(f.data_type),
                _ => {}
            }
        }

        if wanted_samplerate.is_none() || wanted_sampleformat.is_none() {
            return None;
        }

        let format = cpal::Format {
            channels: 2,
            sample_rate: wanted_samplerate.unwrap(),
            data_type: wanted_sampleformat.unwrap(),
        };

        let event_loop = cpal::EventLoop::new();
        let stream_id = event_loop.build_output_stream(&device, &format).unwrap();
        event_loop.play_stream(stream_id);

        let shared_buffer = Arc::new(Mutex::new(Vec::new()));
        let player = CpalPlayer {
            buffer: shared_buffer.clone(),
            sample_rate: wanted_samplerate.unwrap().0,
        };

        std::thread::spawn(move || cpal_thread(event_loop, shared_buffer));

        Some(player)
    }
}

fn cpal_thread(event_loop: cpal::EventLoop, audio_buffer: Arc<Mutex<Vec<(f32, f32)>>>) -> ! {
    event_loop.run(move |_stream_id, stream_data| {
        let mut inbuffer = audio_buffer.lock().unwrap();
        if let cpal::StreamData::Output { buffer } = stream_data {
            let outlen = ::std::cmp::min(buffer.len() / 2, inbuffer.len());
            match buffer {
                cpal::UnknownTypeOutputBuffer::F32(mut outbuffer) => {
                    for (i, (in_l, in_r)) in inbuffer.drain(..outlen).enumerate() {
                        outbuffer[i * 2] = in_l;
                        outbuffer[i * 2 + 1] = in_r;
                    }
                }
                cpal::UnknownTypeOutputBuffer::U16(mut outbuffer) => {
                    for (i, (in_l, in_r)) in inbuffer.drain(..outlen).enumerate() {
                        outbuffer[i * 2] = (in_l * f32::from(std::i16::MAX) + f32::from(std::u16::MAX) / 2.0) as u16;
                        outbuffer[i * 2 + 1] =
                            (in_r * f32::from(std::i16::MAX) + f32::from(std::u16::MAX) / 2.0) as u16;
                    }
                }
                cpal::UnknownTypeOutputBuffer::I16(mut outbuffer) => {
                    for (i, (in_l, in_r)) in inbuffer.drain(..outlen).enumerate() {
                        outbuffer[i * 2] = (in_l * f32::from(std::i16::MAX)) as i16;
                        outbuffer[i * 2 + 1] = (in_r * f32::from(std::i16::MAX)) as i16;
                    }
                }
            }
        }
    });
}

impl AudioPlayer for CpalPlayer {
    fn play(&mut self, buf_left: &[f32], buf_right: &[f32]) {
        debug_assert!(buf_left.len() == buf_right.len());

        let mut buffer = self.buffer.lock().unwrap();

        for (l, r) in buf_left.iter().zip(buf_right) {
            if buffer.len() > self.sample_rate as usize {
                // Do not fill the buffer with more than 1 second of data
                // This speeds up the resync after the turning on and off the speed limiter
                return;
            }
            buffer.push((*l, *r));
        }
    }

    fn samples_rate(&self) -> u32 {
        self.sample_rate
    }

    fn underflowed(&self) -> bool {
        (*self.buffer.lock().unwrap()).is_empty()
    }
}
