extern crate ffmpeg_next as ffmpeg;

use winit::{
    event::{Event, KeyEvent, ElementState, StartCause, WindowEvent},
    event_loop::{EventLoop, ControlFlow},
    window::{WindowBuilder, Fullscreen, WindowLevel},
    keyboard::{Key, NamedKey}
};

use ffmpeg::{
    codec,
    format::{
        input,
        Pixel
    },
    media::Type,
    software::scaling::{context::Context, flag::Flags},
    frame::Video
};

use std::{
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
    path::Path,
    env::current_dir,
    env,
    thread,
    sync::{Arc, Mutex},
    thread::sleep
};

use kira::{
    manager::{
        AudioManager, AudioManagerSettings,
        backend::DefaultBackend,
    },
    sound::static_sound::{StaticSoundData, StaticSoundSettings},
    tween::Tween,
};

#[derive(Default)]
struct Content {
    frames: Vec<Video>,
}

fn main() -> Result<(), impl std::error::Error> {
    let exec_path = current_dir().unwrap();
    let folder = exec_path.to_str().unwrap();
    let video_file = Path::new(folder).join(env::args().nth(1).unwrap_or("BadApple.webm".parse().unwrap()));
    let audio_file = Path::new(folder).join(env::args().nth(2).unwrap_or("BadApple.wav".parse().unwrap()));

    // Winit setup
    let event_loop = EventLoop::new().unwrap();
    let monitor = event_loop.available_monitors().next().expect("No monitor found!");

    let window = Rc::new(
        WindowBuilder::new()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .build(&event_loop)
            .unwrap(),
    );

    let fullscreen = Some(Fullscreen::Borderless(Some(monitor.clone())));
    window.set_fullscreen(fullscreen);
    window.set_title("Bad Apple!");
    window.set_cursor_hittest(false).unwrap();

    //ffmpeg setup
    let fps = Arc::new(Mutex::new(Duration::new(0,0)));
    let fps_clone = fps.clone();

    //Kira setup
    let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).unwrap();

    // Softbuffer setup
    let context = softbuffer::Context::new(window.clone()).unwrap();
    let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

    let frames = Arc::new(Mutex::new(Content::default()));
    let frames_gen = frames.clone();

    let width = monitor.size().width;
    let height = monitor.size().height;

    let _ = thread::spawn(move || {
        ffmpeg::init().unwrap();

        let mut input = input(&video_file).unwrap();
        let video_stream = input.streams().best(Type::Video).unwrap();
        let video_stream_index = video_stream.index();

        let context_decoder = codec::context::Context::from_parameters(video_stream.parameters()).unwrap();

        let mut fps = fps_clone.lock().unwrap();
        let rate = video_stream.avg_frame_rate();
        let rate = (rate.denominator() as f64) / (rate.numerator() as f64);
        let rate = (rate * 1000000000.0) as u64;
        *fps = Duration::from_nanos(rate);
        drop(fps);

        let mut decoder = context_decoder.decoder().video().unwrap();

        decoder.set_threading(ffmpeg::threading::Config {
            kind: ffmpeg::threading::Type::Frame,
            count: 4,
            safe: false,
        });

        let mut scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGB24,
            width,
            height,
            Flags::FAST_BILINEAR,
        ).unwrap();

        let mut packet_iter = input.packets().into_iter();

        loop {
            let mut content = frames_gen.lock().unwrap();
            if content.frames.len() < 50 {
                if let Some((stream, packet)) = packet_iter.next(){
                    if stream.index() == video_stream_index {
                        decoder.send_packet(&packet).unwrap();
                        let mut decoded = Video::empty();
                        decoder.receive_frame(&mut decoded).unwrap();
                        let mut rgb_frame = Video::empty();
                        scaler.run(&decoded, &mut rgb_frame).unwrap();
                        content.frames.push(rgb_frame);
                        drop(content);
                    }
                }
            } else {
                drop(content);
            }
        }
    });

    sleep(Duration::from_secs(1));
    let mut start = Instant::now();
    let mut is_playing = false;

    event_loop.run(move |event, elwt| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                elwt.set_control_flow(ControlFlow::Poll);
                surface.resize(
                    NonZeroU32::new(monitor.size().width).unwrap(),
                    NonZeroU32::new(monitor.size().height).unwrap(),
                ).unwrap();},
            Event::NewEvents(StartCause::Poll) => {
                let elapsed = start.elapsed();
                if elapsed >= *fps.lock().unwrap() {
                    start = Instant::now();
                    window.request_redraw();
                }
            },
            Event::WindowEvent {event, ..} => {
                match event{
                    WindowEvent::CloseRequested => {elwt.exit()},
                    WindowEvent::KeyboardInput {
                        event:
                        KeyEvent {
                            logical_key: key,
                            state: ElementState::Pressed,
                            ..
                        },
                        ..
                    } => match key {
                        Key::Named(NamedKey::Escape) => elwt.exit(),
                        _ => {}
                    },
                    WindowEvent::RedrawRequested => {
                        let mut content = frames.lock().unwrap();
                        if content.frames.len() != 0 {
                            let mut buffer = surface.buffer_mut().unwrap();
                            let data = content.frames[0].data(0);
                            for i in 0..(content.frames[0].width() * content.frames[0].height()) {
                                let r = data[i as usize * 3usize] as u32;
                                let g = data[i as usize * 3usize + 1usize] as u32;
                                let b = data[i as usize * 3usize + 2usize] as u32;
                                buffer[i as usize] = b | (g << 8) | (r << 16);
                            }
                            buffer.present().unwrap();
                            content.frames.remove(0);
                        } else {
                            elwt.exit();
                        }
                        if !is_playing {
                            let sound_data = StaticSoundData::from_file(&audio_file, StaticSoundSettings::new()).unwrap();
                            let mut sound = manager.play(sound_data).unwrap();
                            sound.set_volume(0.1, Tween::default()).unwrap();
                            is_playing = true;
                        }

                    },
                    _ => {}
                }
            }
            _ => {}
        }
    })
}