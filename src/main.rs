// Note: Game BoyTM, Game Boy PocketTM, Super Game BoyTM and Game Boy ColorTM are registered trademarks of
// Nintendo CO., LTD. © 1989 to 1999 by Nintendo CO., LTD.
use gameboy::convention::Term;
use gameboy::gpu::{SCREEN_H, SCREEN_W};
use gameboy::joypad::JoypadKey;
use gameboy::motherboard::MotherBoard;
use gameboy::sound::AudioPlayer;
use std::error::Error;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;

const EXITCODE_SUCCESS: i32 = 0;
const EXITCODE_CPULOADFAILS: i32 = 2;

#[derive(Default)]
struct RenderOptions {
    pub linear_interpolation: bool,
}

enum GBEvent {
    KeyUp(JoypadKey),
    KeyDown(JoypadKey),
    SpeedUp,
    SpeedDown,
}

fn main() {
    let exit_status = real_main();
    if exit_status != EXITCODE_SUCCESS {
        std::process::exit(exit_status);
    }
}

fn real_main() -> i32 {
    let matches = clap::App::new("gameboy")
        .version("0.1")
        .author("Mathijs van de Nes")
        .about("A Gameboy Colour emulator written in Rust")
        .arg(
            clap::Arg::with_name("filename")
                .help("Sets the ROM file to load")
                .required(true),
        )
        .arg(
            clap::Arg::with_name("serial")
                .help("Prints the data from the serial port to stdout")
                .short("s")
                .long("serial"),
        )
        .arg(
            clap::Arg::with_name("printer")
                .help("Emulates a gameboy printer")
                .short("p")
                .long("printer"),
        )
        .arg(
            clap::Arg::with_name("classic")
                .help("Forces the emulator to run in classic Gameboy mode")
                .short("c")
                .long("classic"),
        )
        .arg(
            clap::Arg::with_name("scale")
                .help("Sets the scale of the interface. Default: 2")
                .short("x")
                .long("scale")
                .validator(|s| match s.parse::<u32>() {
                    Err(e) => Err(format!("Could not parse scale: {}", e.description())),
                    Ok(s) if s < 1 => Err("Scale must be at least 1".to_owned()),
                    Ok(s) if s > 8 => Err("Scale may be at most 8".to_owned()),
                    Ok(..) => Ok(()),
                })
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("audio")
                .help("Enables audio")
                .short("a")
                .long("audio"),
        )
        .get_matches();

    let opt_classic = matches.is_present("classic");
    let opt_audio = matches.is_present("audio");
    let filename = matches.value_of("filename").unwrap();
    let scale = matches.value_of("scale").unwrap_or("2").parse::<u32>().unwrap();

    let cpu = construct_cpu(filename, opt_classic);
    if cpu.is_none() {
        return EXITCODE_CPULOADFAILS;
    }
    let mut cpu = cpu.unwrap();
    if opt_audio {
        let player = CpalPlayer::get();
        match player {
            Some(v) => cpu.enable_audio(Box::new(v) as Box<AudioPlayer>),
            None => {
                warn("Could not open audio device");
                return EXITCODE_CPULOADFAILS;
            }
        }
    }
    let romname = cpu.romname();

    let (sender1, receiver1) = mpsc::channel();
    let (sender2, receiver2) = mpsc::sync_channel(1);

    // Force winit to use x11 instead of wayland, wayland is not fully supported yet by winit.
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");

    let mut eventsloop = glium::glutin::EventsLoop::new();
    let window_builder = glium::glutin::WindowBuilder::new()
        .with_dimensions(glium::glutin::dpi::LogicalSize::from((
            SCREEN_W as u32,
            SCREEN_H as u32,
        )))
        .with_title("RBoy - ".to_owned() + &romname);
    let context_builder = glium::glutin::ContextBuilder::new();
    let display = glium::backend::glutin::Display::new(window_builder, context_builder, &eventsloop).unwrap();
    set_window_size(&**display.gl_window(), scale);

    let mut texture = glium::texture::texture2d::Texture2d::empty_with_format(
        &display,
        glium::texture::UncompressedFloatFormat::U8U8U8,
        glium::texture::MipmapsOption::NoMipmap,
        SCREEN_W as u32,
        SCREEN_H as u32,
    )
    .unwrap();

    let mut renderoptions = <RenderOptions as Default>::default();

    let cputhread = thread::spawn(move || run_cpu(cpu, sender2, receiver1));

    loop {
        let mut stop = false;
        eventsloop.poll_events(|ev| {
            use glium::glutin::ElementState::{Pressed, Released};
            use glium::glutin::VirtualKeyCode;
            use glium::glutin::{Event, KeyboardInput, WindowEvent};

            match ev {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => stop = true,
                    WindowEvent::KeyboardInput { input, .. } => match input {
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        } => stop = true,
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Key1),
                            ..
                        } => set_window_size(&**display.gl_window(), 1),
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(VirtualKeyCode::R),
                            ..
                        } => set_window_size(&**display.gl_window(), scale),
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(VirtualKeyCode::LShift),
                            ..
                        } => {
                            let _ = sender1.send(GBEvent::SpeedUp);
                        }
                        KeyboardInput {
                            state: Released,
                            virtual_keycode: Some(VirtualKeyCode::LShift),
                            ..
                        } => {
                            let _ = sender1.send(GBEvent::SpeedDown);
                        }
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(VirtualKeyCode::T),
                            ..
                        } => {
                            renderoptions.linear_interpolation = !renderoptions.linear_interpolation;
                        }
                        KeyboardInput {
                            state: Pressed,
                            virtual_keycode: Some(glutinkey),
                            ..
                        } => {
                            if let Some(key) = glutin_to_keypad(glutinkey) {
                                let _ = sender1.send(GBEvent::KeyDown(key));
                            }
                        }
                        KeyboardInput {
                            state: Released,
                            virtual_keycode: Some(glutinkey),
                            ..
                        } => {
                            if let Some(key) = glutin_to_keypad(glutinkey) {
                                let _ = sender1.send(GBEvent::KeyUp(key));
                            }
                        }
                        _ => (),
                    },
                    _ => (),
                },
                _ => (),
            }
        });

        if stop == true {
            break;
        }

        match receiver2.recv() {
            Ok(data) => recalculate_screen(&display, &mut texture, &*data, &renderoptions),
            Err(..) => break, // Remote end has hung-up
        }
    }

    drop(sender1);
    let _ = cputhread.join();

    EXITCODE_SUCCESS
}

fn glutin_to_keypad(key: glium::glutin::VirtualKeyCode) -> Option<JoypadKey> {
    use glium::glutin::VirtualKeyCode;
    match key {
        VirtualKeyCode::Z => Some(JoypadKey::A),
        VirtualKeyCode::X => Some(JoypadKey::B),
        VirtualKeyCode::Up => Some(JoypadKey::Up),
        VirtualKeyCode::Down => Some(JoypadKey::Down),
        VirtualKeyCode::Left => Some(JoypadKey::Left),
        VirtualKeyCode::Right => Some(JoypadKey::Right),
        VirtualKeyCode::Space => Some(JoypadKey::Select),
        VirtualKeyCode::Return => Some(JoypadKey::Start),
        _ => None,
    }
}

fn recalculate_screen(
    display: &glium::Display,
    texture: &mut glium::texture::texture2d::Texture2d,
    datavec: &[u8],
    renderoptions: &RenderOptions,
) {
    use glium::Surface;

    let interpolation_type = if renderoptions.linear_interpolation {
        glium::uniforms::MagnifySamplerFilter::Linear
    } else {
        glium::uniforms::MagnifySamplerFilter::Nearest
    };

    let rawimage2d = glium::texture::RawImage2d {
        data: std::borrow::Cow::Borrowed(datavec),
        width: SCREEN_W as u32,
        height: SCREEN_H as u32,
        format: glium::texture::ClientFormat::U8U8U8,
    };
    texture.write(
        glium::Rect {
            left: 0,
            bottom: 0,
            width: SCREEN_W as u32,
            height: SCREEN_H as u32,
        },
        rawimage2d,
    );

    // We use a custom BlitTarget to transform OpenGL coordinates to row-column coordinates
    let target = display.draw();
    let (target_w, target_h) = target.get_dimensions();
    texture.as_surface().blit_whole_color_to(
        &target,
        &glium::BlitTarget {
            left: 0,
            bottom: target_h,
            width: target_w as i32,
            height: -(target_h as i32),
        },
        interpolation_type,
    );
    target.finish().unwrap();
}

fn warn(message: &'static str) {
    use std::io::Write;
    let _ = write!(&mut std::io::stderr(), "{}\n", message);
}

fn construct_cpu(filename: &str, classic_mode: bool) -> Option<Box<MotherBoard>> {
    let c = match classic_mode {
        true => MotherBoard::power_up(Term::GB, filename),
        false => MotherBoard::power_up(Term::GBC, filename),
    };
    Some(Box::new(c))
}

fn run_cpu(mut cpu: Box<MotherBoard>, sender: SyncSender<Vec<u8>>, receiver: Receiver<GBEvent>) {
    let periodic = timer_periodic(16);
    let mut limit_speed = true;

    let waitticks = (4194304f64 / 1000.0 * 16.0).round() as u32;
    let mut ticks = 0;

    'outer: loop {
        while ticks < waitticks {
            ticks += cpu.do_cycle();
            if cpu.check_and_reset_gpu_updated() {
                let data = cpu.get_gpu_data();
                if let Err(TrySendError::Disconnected(..)) = sender.try_send(data) {
                    break 'outer;
                }
            }
        }

        ticks -= waitticks;

        'recv: loop {
            match receiver.try_recv() {
                Ok(event) => match event {
                    GBEvent::KeyUp(key) => cpu.keyup(key),
                    GBEvent::KeyDown(key) => cpu.keydown(key),
                    GBEvent::SpeedUp => limit_speed = false,
                    GBEvent::SpeedDown => {
                        limit_speed = true;
                        cpu.sync_audio();
                    }
                },
                Err(TryRecvError::Empty) => break 'recv,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }

        if limit_speed {
            let _ = periodic.recv();
        }
    }

    cpu.mmu.cartridge.sav();
}

fn timer_periodic(ms: u64) -> Receiver<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        if tx.send(()).is_err() {
            break;
        }
    });
    rx
}

fn set_window_size(window: &glium::glutin::Window, scale: u32) {
    use glium::glutin::dpi::{LogicalSize, PhysicalSize};

    let dpi = window.get_hidpi_factor();

    let physical_size = PhysicalSize::from((SCREEN_W as u32 * scale, SCREEN_H as u32 * scale));
    let logical_size = LogicalSize::from_physical(physical_size, dpi);

    window.set_inner_size(logical_size);
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
                Some(cpal::SampleRate(r)) if r < f.max_sample_rate.0 && r < 192000 => {
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

        thread::spawn(move || cpal_thread(event_loop, shared_buffer));

        Some(player)
    }
}

fn cpal_thread(event_loop: cpal::EventLoop, audio_buffer: Arc<Mutex<Vec<(f32, f32)>>>) -> ! {
    event_loop.run(move |_stream_id, stream_data| {
        let mut inbuffer = audio_buffer.lock().unwrap();
        match stream_data {
            cpal::StreamData::Output { buffer } => {
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
                            outbuffer[i * 2] = (in_l * (std::i16::MAX as f32) + (std::u16::MAX as f32) / 2.0) as u16;
                            outbuffer[i * 2 + 1] =
                                (in_r * (std::i16::MAX as f32) + (std::u16::MAX as f32) / 2.0) as u16;
                        }
                    }
                    cpal::UnknownTypeOutputBuffer::I16(mut outbuffer) => {
                        for (i, (in_l, in_r)) in inbuffer.drain(..outlen).enumerate() {
                            outbuffer[i * 2] = (in_l * (std::i16::MAX as f32)) as i16;
                            outbuffer[i * 2 + 1] = (in_r * (std::i16::MAX as f32)) as i16;
                        }
                    }
                }
            }
            _ => (),
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
        (*self.buffer.lock().unwrap()).len() == 0
    }
}
