extern crate repng;
extern crate scrap;
#[macro_use]
extern crate log;
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use image::{GenericImage, GenericImageView, ImageBuffer, Pixel, Primitive};
use scrap::{Capturer, Display};
use std::fs::File;
use std::io::ErrorKind::WouldBlock;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{fs, thread, time};

/// Capture screenshots at regular intervals
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Do not compress using pngquant
    #[clap(short, long)]
    no_compression: bool,

    /// Split each image into its own file, do not combine
    #[clap(short, long)]
    split_images: bool,

    /// Output directory
    #[clap(short, long, default_value = "./")]
    output_dir: String,

    /// Interval in seconds, 0 for once off
    #[clap(short, long, default_value_t = 30)]
    interval: u16,

    /// Generate video
    #[clap(subcommand)]
    output: Option<Generate>,

    /// File prefix
    #[clap(short, long, default_value = "screen")]
    file_prefix: String,
}
#[derive(Subcommand, Debug)]
enum Generate {
    #[clap()]
    Generate {
        /// Day of images to process into video
        /// Format: YYYY-MM-DD
        #[clap(short, long)]
        date: NaiveDate,
    },
}

fn main() {
    env_logger::init();

    let args = Args::parse();
    let output_dir = Path::new(&args.output_dir);
    let compress = !args.no_compression;
    let interval = args.interval;
    let single_image = !args.split_images;
    let file_prefix = args.file_prefix;

    if let Some(output) = &args.output {
        match output {
            Generate::Generate { date } => {
                generate_mp4(date, output_dir, file_prefix);
                return;
            }
        }
    }

    if interval == 0 {
        capture_displays(output_dir, compress, single_image, &file_prefix);
        return;
    }
    debug!("Capturing images every {} seconds", interval);
    let delay = time::Duration::from_secs(interval as u64);
    loop {
        capture_displays(output_dir, compress, single_image, &file_prefix);
        debug!("Sleeping for {} seconds", interval);
        sleep(delay);
    }
}

fn generate_mp4(date: &NaiveDate, output_dir: &Path, file_prefix: String) {
    let _cmd = Command::new("ffmpeg")
        .arg("-framerate")
        .arg("2")
        .arg("-pattern_type")
        .arg("glob")
        .arg("-i")
        .arg(format!(
            "{}/screen_{}*.png",
            output_dir.to_string_lossy(),
            date.format("%Y%m%d")
        ))
        .arg("-c:v")
        .arg("libx265")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(output_dir.join(format!("{}_{}.mp4", file_prefix, date.format("%Y%m%d"))))
        .output()
        .expect("Failed to generate clip");
    trace!("{:?}", _cmd);
}

fn capture_displays(output_dir: &Path, compress: bool, single_image: bool, file_prefix: &str) {
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
            let filename = output_dir.join(format!(
                "{}_{}_{}.png",
                file_prefix,
                now.format("%Y%m%d_%H%M%S"),
                w
            ));
            image_filenames.push(filename.clone());
            repng::encode(
                File::create(&filename).unwrap(),
                w as u32,
                h as u32,
                &bitflipped,
            )
            .unwrap();

            if compress && !single_image {
                debug!(
                    "Compressing individual image {}",
                    filename.to_string_lossy()
                );
                compress_image(&filename);
            }
            break;
        }
    }
    if single_image {
        debug!("Combining {} display images", image_filenames.len());
        let images = image_filenames
            .iter()
            .map(|i| image::open(i).unwrap())
            .collect::<Vec<_>>();

        let merged_file = output_dir.join(format!(
            "{}_{}.png",
            file_prefix,
            now.format("%Y%m%d_%H%M%S")
        ));
        h_concat(&images).save(&merged_file).unwrap();
        if compress {
            debug!(
                "Compressing combined image {}",
                merged_file.to_string_lossy()
            );
            compress_image(&merged_file);
        }
        for image in image_filenames {
            debug!(
                "Deleting individual display file {}",
                image.to_string_lossy()
            );
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
        .arg("--output")
        .arg(path.clone())
        .arg(path)
        .output()
        .expect("Failed to compress");
    trace!("{:?}", _cmd);
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
