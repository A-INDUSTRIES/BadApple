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
    software::scaling::{context::Context, flag::Flags}};

use std::{
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
    path::Path,
};
use ffmpeg::frame::Video;

use kira::{
    manager::{
        AudioManager, AudioManagerSettings,
        backend::DefaultBackend,
    },
    sound::static_sound::{StaticSoundData, StaticSoundSettings},
    tween::Tween,
};

fn main() -> Result<(), impl std::error::Error> {
    let video_file = Path::new("BadApple.webm");
    let audio_file = Path::new("BadApple.wav");

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
    ffmpeg::init().unwrap();
    let mut input = input(&video_file).unwrap();
    let video_stream = input.streams().best(Type::Video).unwrap();
    let video_stream_index = video_stream.index();

    let context_decoder = codec::context::Context::from_parameters(video_stream.parameters()).unwrap();
    let mut decoder = context_decoder.decoder().video().unwrap();

    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        monitor.size().width,
        monitor.size().height,
        Flags::BILINEAR,
    ).unwrap();

    let fps = video_stream.avg_frame_rate().invert();
    let fps = (fps.numerator() as f64) / (fps.denominator() as f64);
    let fps = (fps * 1000000000.0) as u64;
    let fps = Duration::from_nanos(fps);

    let mut packet_iter = input.packets().into_iter();

    //Kira setup
    let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).unwrap();
    let sound_data = StaticSoundData::from_file(audio_file, StaticSoundSettings::new()).unwrap();
    let mut sound = manager.play(sound_data).unwrap();
    sound.pause(Tween::default()).expect("Could not pause");
    sound.set_volume(0.1, Tween::default()).unwrap();

    // Softbuffer setup
    let context = softbuffer::Context::new(window.clone()).unwrap();
    let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

    let mut start = Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                elwt.set_control_flow(ControlFlow::Poll);
                surface.resize(
                    NonZeroU32::new(monitor.size().width).unwrap(),
                    NonZeroU32::new(monitor.size().height).unwrap(),
                ).unwrap();
                sound.resume(Tween::default()).unwrap_or(());},
            Event::NewEvents(StartCause::Poll) => {
                if start.elapsed() >= fps {
                    window.request_redraw();
                    start = Instant::now();
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
                        loop {
                            if let Some((stream, packet)) = packet_iter.next(){
                                if stream.index() == video_stream_index {
                                    decoder.send_packet(&packet).unwrap();
                                    break;
                                }
                            }
                            else {
                                elwt.exit();
                                break;
                            }
                        }
                        let mut decoded = Video::empty();
                        decoder.receive_frame(&mut decoded).unwrap();
                        let mut rgb_frame = Video::empty();
                        scaler.run(&decoded, &mut rgb_frame).unwrap();

                        let mut buffer = surface.buffer_mut().unwrap();
                        let data = rgb_frame.data(0);

                        for i in 0..(rgb_frame.width() * rgb_frame.height()) {
                            let r= data[i as usize * 3usize] as u32;
                            buffer[i as usize] = r | (r << 8) | (r << 16);
                        }
                        buffer.present().unwrap();
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    })
}