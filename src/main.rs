extern crate repng;
extern crate scrap;
use chrono::NaiveDate;
use image::{GenericImage, GenericImageView, ImageBuffer, Pixel, Primitive};
use scrap::{Capturer, Display};
use std::fs::File;
use std::io::ErrorKind::WouldBlock;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{fs, thread, time};

use clap::Parser;

/// Capture screenshots at regular intervals
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Compress using pngquant
    #[clap(short, long)]
    compress: bool,

    /// Combine all screens into a single image
    #[clap(short, long)]
    single_image: bool,

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
    let single_image = args.single_image;

    if let Some(generate_video) = args.generate_video {
        eprintln!("video = {}", generate_video.format("%Y%m%d"));
        let _cmd = Command::new("ffmpeg")
            .arg("-framerate")
            .arg("2")
            .arg("-pattern_type")
            .arg("glob")
            .arg("-i")
            .arg(format!(
                "{}/screen_{}*.png",
                output_dir.to_string_lossy(),
                generate_video.format("%Y%m%d")
            ))
            .arg("-c:v")
            .arg("libx265")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(output_dir.join(format!("screen_{}.mp4", generate_video.format("%Y%m%d"))))
            .output()
            .expect("Failed to generate clip");
        return;
    }

    if interval == 0 {
        capture_displays(output_dir, compress, single_image);
        return;
    }
    let delay = time::Duration::from_secs(interval as u64);
    loop {
        capture_displays(output_dir, compress, single_image);
        sleep(delay);
    }
}

fn capture_displays(output_dir: &Path, compress: bool, single_image: bool) {
    let one_second = Duration::new(1, 0);
    let one_frame = one_second / 60;
    let displays = Display::all().expect("Couldn't find primary display.");

    let now = chrono::Local::now();
    let mut image_filenames = vec![];
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
            let filename =
                output_dir.join(format!("screen_{}_{}.png", now.format("%Y%m%d_%H%M%S"), w));
            image_filenames.push(filename.clone());
            repng::encode(
                File::create(&filename).unwrap(),
                w as u32,
                h as u32,
                &bitflipped,
            )
            .unwrap();

            if compress && !single_image {
                compress_image(&filename);
            }
            break;
        }
    }
    if single_image {
        let images = image_filenames
            .iter()
            .map(|i| image::open(i).unwrap())
            .collect::<Vec<_>>();

        let merged_file = output_dir.join(format!("screen_{}.png", now.format("%Y%m%d_%H%M%S")));
        h_concat(&images).save(&merged_file).unwrap();
        if compress {
            compress_image(&merged_file);
        }
        for image in image_filenames {
            fs::remove_file(image).expect("Failed to delete screen image");
        }
    }
}

fn compress_image(path: &PathBuf) {
    let _cmd = Command::new("pngquant")
        .arg("--force")
        .arg("--skip-if-larger")
        .arg("--quality")
        .arg("40-60")
        .arg(&format!("--output"))
        .arg(path.clone())
        .arg(path)
        .output()
        .expect("Failed to compress");
}

/// Concatenate horizontally images with the same pixel type.
fn h_concat<I, P, S>(images: &[I]) -> ImageBuffer<P, Vec<S>>
where
    I: GenericImageView<Pixel = P>,
    P: Pixel<Subpixel = S> + 'static,
    S: Primitive + 'static,
{
    // The final width is the sum of all images width.
    let img_width_out: u32 = images.iter().map(|im| im.width()).sum();

    // The final height is the maximum height from the input images.
    let img_height_out: u32 = images.iter().map(|im| im.height()).max().unwrap_or(0);

    // Initialize an image buffer with the appropriate size.
    let mut imgbuf = image::ImageBuffer::new(img_width_out, img_height_out);
    let mut accumulated_width = 0;

    // Copy each input image at the correct location in the output image.
    for img in images {
        imgbuf.copy_from(img, accumulated_width, 0).unwrap();
        accumulated_width += img.width();
    }

    imgbuf
}
