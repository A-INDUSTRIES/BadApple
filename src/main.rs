extern crate ffmpeg_next as ffmpeg;

use winit::{
    event::{Event, KeyEvent, ElementState, StartCause, WindowEvent},
    event_loop::{EventLoop, ControlFlow},
    window::{WindowBuilder, Fullscreen},
    keyboard::{Key, NamedKey}
};

use std::{env, num::NonZeroU32, ops::Add, rc::Rc, time::{Duration, Instant}, path::Path};

use image::GenericImageView;
use image::imageops::FilterType;

const FPS: u32 = 33000000;

fn main() -> Result<(), impl std::error::Error> {
    //ffmpeg setup
    ffmpeg::init().unwrap();

    match ffmpeg::format::input(&std::path::Path::new("src/BadApple.webm")) {
        Ok(context) => {
            for (k,v) in context.metadata().iter() {
                println!("{} : {}", k, v);
            }
            println!("{}", context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE))
        }
        _ => {}
    }
    // Winit setup
    let event_loop = EventLoop::new().unwrap();
    let monitor = event_loop.available_monitors().next().expect("No monitor found!");

    let window = Rc::new(
        WindowBuilder::new()
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .build(&event_loop)
        .unwrap(),
    );

    let fullscreen = Some(Fullscreen::Borderless(Some(monitor.clone())));
    window.set_fullscreen(fullscreen);
    window.set_title("Bad Apple!");

    // Softbuffer setup
    let image = image::load_from_memory(include_bytes!("image.png")).unwrap();
    let image = image.resize_to_fill(monitor.size().width, monitor.size().height, FilterType::Nearest);
    let start_position = (monitor.size().height - image.height())/2;
    let context = softbuffer::Context::new(window.clone()).unwrap();
    let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

    //let start = Instant::now();

    event_loop.run(move |event, elwt| {
            match event {
                Event::NewEvents(StartCause::Init) => {elwt
                    .set_control_flow(
                        ControlFlow::WaitUntil(Instant::now()
                            .add(Duration::new(0, FPS))
                    ));},
                Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                    window.request_redraw()},
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
                        surface.resize(
                            NonZeroU32::new(image.width()).unwrap(),
                            NonZeroU32::new(image.height() + (start_position * 2)).unwrap(),
                        ).unwrap();

                        let mut buffer = surface.buffer_mut().unwrap();
                        let width = image.width() as usize;
                        for (x,y,pixel) in image.pixels() {
                            let mut r:u32 = 0;
                            if pixel.0[0] > 100 {
                                r = 255;
                            }
                            buffer[(start_position + y) as usize * width + x as usize] = r | (r << 8) | (r << 16);
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