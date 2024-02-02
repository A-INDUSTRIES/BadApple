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
        Pixel,
        Sample as ffSample,
        sample::Type as SampleType
    },
    media::Type,
    software::scaling::{context::Context, flag::Flags},
    software::resampling::{context::Context as ResamplingContext},
    frame::{Video, Audio}
};

use std::{
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
    path::{Path, PathBuf},
    env,
    thread,
    sync::{Arc, Mutex},
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample,
    SampleFormat
};

use ringbuf::RingBuffer;

trait SampleFormatConversion {
    fn as_ffmpeg_sample(&self) -> ffSample;
}

impl SampleFormatConversion for SampleFormat {
    fn as_ffmpeg_sample(&self) -> ffSample {
        match self {
            Self::I16 => ffSample::I16(SampleType::Packed),
            Self::U16 => {
                panic!("ffmpeg resampler doesn't support u16")
            },
            Self::F32 => ffSample::F32(SampleType::Packed),
            _ => ffSample::U8(SampleType::Packed)
        }
    }
}

fn main() -> Result<(), impl std::error::Error> {
    let (file, monitor_index) = parse_args();
    let exec_path = env::current_dir().unwrap();
    let folder = exec_path.to_str().unwrap();
    let video_file = Path::new(folder).join(file);

    // Winit setup
    let event_loop = EventLoop::new().unwrap();
    let monitor = event_loop.available_monitors()
        .nth(monitor_index)
        .expect("No monitor found!");
    let fullscreen = Some(Fullscreen::Borderless(Some(monitor.clone())));

    let window = Rc::new(
        WindowBuilder::new()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .build(&event_loop)
            .unwrap(),
    );
    window.set_fullscreen(fullscreen);
    window.set_title("Bad Apple!");
    window.set_cursor_hittest(false).unwrap();

    // Softbuffer setup
    let context = softbuffer::Context::new(window.clone()).unwrap();
    let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

    //Audio thread setup
    let is_playing = Arc::new(Mutex::new(false));
    let thread_playing = is_playing.clone();
    run_audio_thread(&video_file, thread_playing);

    //Frame thread setup
    let fps = Arc::new(Mutex::new(Duration::new(0,0)));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let thread_width = monitor.size().width;
    let thread_height = monitor.size().height;
    run_frame_thread(&video_file, &frames, &fps, thread_width, thread_height);

    let mut start = Instant::now();

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
                        if content.len() != 0 && content.len() > 69 {
                            let mut buffer = surface.buffer_mut().unwrap();
                            let data = content[0].data(0);
                            for i in 0..(content[0].width() * content[0].height()) {
                                let r = data[i as usize * 3usize] as u32;
                                let g = data[i as usize * 3usize + 1usize] as u32;
                                let b = data[i as usize * 3usize + 2usize] as u32;
                                buffer[i as usize] = b | (g << 8) | (r << 16);
                            }
                            buffer.present().unwrap();
                            content.remove(0);
                            if !*is_playing.lock().unwrap() {
                                *is_playing.lock().unwrap() = true;
                            }
                        } else if *is_playing.lock().unwrap() {
                            elwt.exit(); // If there are no more frames to present, exits the app.
                        }
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    })
}

fn parse_args() -> (String, usize) {
    let mut file = String::from("BadApple.webm");
    let mut monitor = 0;
    for arg in env::args().into_iter().skip(1) {
        match arg.parse::<usize>() {
            Ok(string) => monitor = string,
            Err(..) => file = arg,
        }
    }
    (file, monitor)
}

fn run_frame_thread(file: &PathBuf, frames: &Arc<Mutex<Vec<Video>>>, fps: &Arc<Mutex<Duration>>, width: u32, height: u32) {
    let file = file.clone();
    let frames = frames.clone();
    let fps = fps.clone();

    let _ = thread::spawn(move || {
        ffmpeg::init().unwrap();

        let mut input = input(&file).unwrap();
        let video_stream = input.streams().best(Type::Video).unwrap();
        let video_stream_index = video_stream.index();

        let context_decoder = codec::context::Context::from_parameters(video_stream.parameters()).unwrap();

        let mut fps = fps.lock().unwrap();
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
            let mut content = frames.lock().unwrap();
            if content.len() <= 70 {
                if let Some((stream, packet)) = packet_iter.next(){
                    if stream.index() == video_stream_index {
                        decoder.send_packet(&packet).unwrap();
                        let mut decoded = Video::empty();
                        decoder.receive_frame(&mut decoded).unwrap();
                        let mut rgb_frame = Video::empty();
                        scaler.run(&decoded, &mut rgb_frame).unwrap();
                        content.push(rgb_frame);
                        drop(content);
                    }
                }
            } else {
                drop(content);
            }
        }
    });
}

fn run_audio_thread(file: &PathBuf, is_playing: Arc<Mutex<bool>>) {
    //Cpal setup
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let mut supported_configs_range = device.supported_output_configs().unwrap();
    let supported_config = supported_configs_range.next().unwrap().with_max_sample_rate();

    let file = file.clone();
    let stream_config = supported_config.clone();

    let _ = thread::spawn(move || {
        ffmpeg::init().unwrap();

        let mut ictx = input(&file).unwrap();

        let audio = ictx
            .streams()
            .best(Type::Audio)
            .ok_or(ffmpeg::Error::StreamNotFound).unwrap();
        let audio_stream_index = audio.index();

        let mut audio_decoder = audio.codec().decoder().audio().unwrap();

        let mut resampler = ResamplingContext::get(
            audio_decoder.format(),
            audio_decoder.channel_layout(),
            audio_decoder.rate(),
            stream_config.sample_format().as_ffmpeg_sample(),
            audio_decoder.channel_layout(),
            stream_config.sample_rate().0
        ).unwrap();

        let buffer = RingBuffer::<f32>::new(8192);
        let (mut producer, mut consumer) = buffer.split();

        let audio_stream = match stream_config.sample_format() {
            SampleFormat::F32 => device.build_output_stream(&stream_config.into(), move |data: &mut [f32], cbinfo| {
                write_audio(data, &mut consumer, &cbinfo)
            }, |err| {
                eprintln!("audio error: {}", err)
            }, None),
            SampleFormat::I16 => panic!("i16 output format unimplemented"),
            SampleFormat::U16 => panic!("u16 output format unimplemented"),
            _ => panic!("Other output format unimplemented")
        }.unwrap();

        let mut receive_and_queue_audio_frames =
            |decoder: &mut ffmpeg::decoder::Audio| -> Result<(), ffmpeg::Error> {
                let mut decoded = Audio::empty();

                while decoder.receive_frame(&mut decoded).is_ok() {
                    let mut resampled = Audio::empty();
                    resampler.run(&decoded, &mut resampled)?;
                    let both_channels = packed(&resampled);
                    while producer.remaining() < both_channels.len() {
                        thread::sleep(Duration::from_millis(10));
                    }
                    producer.push_slice(both_channels);
                }
                Ok(())
            };

        // Wait for first frame and play
        loop {
            let is_playing = is_playing.lock().unwrap();
            if *is_playing {
                audio_stream.play().unwrap();
                break;
            }
        }

        for (stream, packet) in ictx.packets() {
            if stream.index() == audio_stream_index {
                audio_decoder.send_packet(&packet).unwrap();
                receive_and_queue_audio_frames(&mut audio_decoder).unwrap();
            }
        }
    });
}

pub fn packed<T: ffmpeg::frame::audio::Sample>(frame: &Audio) -> &[T] {
    if !frame.is_packed() {
        panic!("data is not packed");
    }

    if !<T as ffmpeg::frame::audio::Sample>::is_valid(frame.format(), frame.channels()) {
        panic!("unsupported type");
    }

    unsafe { std::slice::from_raw_parts((*frame.as_ptr()).data[0] as *const T, frame.samples() * frame.channels() as usize) }
}

fn write_audio<T: Sample>(data: &mut [T], samples: &mut ringbuf::Consumer<T>, _: &cpal::OutputCallbackInfo) {
    for d in data {
        // copy as many samples as we have.
        // if we run out, write silence
        match samples.pop() {
            Some(sample) => *d = sample,
            None => *d = Sample::EQUILIBRIUM
        }
    }
}