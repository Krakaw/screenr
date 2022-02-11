extern crate repng;
extern crate scrap;
use chrono::NaiveDate;
use scrap::{Capturer, Display};
use std::fs::File;
use std::io::ErrorKind::WouldBlock;
use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{thread, time};

use clap::Parser;

/// Capture screenshots at regular intervals
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Compress using pngquant
    #[clap(short, long)]
    compress: bool,

    /// Output directory
    #[clap(short, long, default_value = "./")]
    output_dir: String,

    /// Interval in seconds, 0 for once off
    #[clap(short, long, default_value_t = 0)]
    interval: u16,

    /// Generate video
    #[clap(short, long)]
    generate_video: Option<NaiveDate>,
}

fn main() {
    let args = Args::parse();
    let output_dir = Path::new(&args.output_dir);
    let compress = args.compress;
    let interval = args.interval;
    if let Some(generate_video) = args.generate_video {
        eprintln!("video = {}", generate_video.format("%Y%m%d"));
        return;
    }

    if interval == 0 {
        capture_displays(output_dir, compress);
        return;
    }
    let delay = time::Duration::from_secs(interval as u64);
    loop {
        capture_displays(output_dir, compress);
        sleep(delay);
    }
}

fn capture_displays(output_dir: &Path, compress: bool) {
    let one_second = Duration::new(1, 0);
    let one_frame = one_second / 60;
    let displays = Display::all().expect("Couldn't find primary display.");

    let now = chrono::Local::now();
    for display in displays.into_iter() {
        let mut capturer = Capturer::new(display).expect("Couldn't begin capture.");
        loop {
            // Wait until there's a frame.
            let (w, h) = (capturer.width(), capturer.height());
            let buffer = match capturer.frame() {
                Ok(buffer) => buffer,
                Err(error) => {
                    if error.kind() == WouldBlock {
                        // Keep spinning.
                        thread::sleep(one_frame);
                        continue;
                    } else {
                        panic!("Error: {}", error);
                    }
                }
            };

            // Flip the ARGB image into a BGRA image.
            let mut bitflipped = Vec::with_capacity(w * h * 4);
            let stride = buffer.len() / h;

            for y in 0..h {
                for x in 0..w {
                    let i = stride * y + 4 * x;
                    bitflipped.extend_from_slice(&[buffer[i + 2], buffer[i + 1], buffer[i], 255]);
                }
            }

            // Save the image.
            let filename = Path::new(&output_dir).join(format!(
                "screen_{}_{}.png",
                now.format("%Y%m%d_%H%M%S"),
                w
            ));
            repng::encode(
                File::create(&filename).unwrap(),
                w as u32,
                h as u32,
                &bitflipped,
            )
            .unwrap();

            if compress {
                let _cmd = Command::new("pngquant")
                    .arg("--force")
                    .arg("--skip-if-larger")
                    .arg("--quality")
                    .arg("50")
                    .arg(&format!("--output"))
                    .arg(filename.clone())
                    .arg(filename)
                    .output()
                    .expect("Failed to compress");
            }
            break;
        }
    }
}
