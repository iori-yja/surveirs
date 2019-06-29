extern crate clap;
extern crate image;
extern crate reqwest;
extern crate rscam;

use clap::{App, Arg};
use image::gif;
use image::imageops::filter3x3;
use image::jpeg::JPEGDecoder;
use image::{
    ColorType, ConvertBuffer, GrayImage, ImageBuffer, ImageDecoder, ImageResult, Luma, Pixel, Rgb,
};
use rscam::{Camera, Config};
use std::fs;
use std::io::{BufReader, BufWriter, Read};
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

fn and<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();

    let flat_a = ima.as_flat_samples();
    let flat_b = imb.as_flat_samples();

    let (bufa, bufb) = match (flat_a.image_slice(), flat_b.image_slice()) {
        (Some(bufa), Some(bufb)) => (bufa, bufb),
        _ => panic!("invalid buffer"),
    };

    for (a, b) in bufa.iter().zip(bufb.iter()) {
        let ax = (*a) as i32;
        let bx = (*b) as i32;
        let p = if (ax - bx).abs() < 50 { ax } else { 0 };
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
}

fn reduce_color<P>(im: &ImageBuffer<P, Vec<u8>>, step: u8) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let mut dst = Vec::with_capacity(im.len());
    let dim = im.dimensions();

    let flat = im.as_flat_samples();

    for p in flat.image_slice().unwrap() {
        dst.push(*p >> step);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn brightness<P>(im: &ImageBuffer<P, Vec<u8>>) -> i32
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let br = im
        .pixels()
        .fold(0 as u64, |b, p| b + p.to_luma().data[0] as u64);
    return (br >> 6) as i32;
}

fn diff<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();
    let bria = brightness(ima);
    let brib = brightness(imb);

    let flat_a = ima.as_flat_samples();
    let flat_b = imb.as_flat_samples();

    let (bufa, bufb) = match (flat_a.image_slice(), flat_b.image_slice()) {
        (Some(bufa), Some(bufb)) => (bufa, bufb),
        _ => panic!("invalid buffer"),
    };

    for (a, b) in bufa.iter().zip(bufb.iter()) {
        let ax = (*a) as i32 * brib;
        let bx = (*b) as i32 * bria;
        let p = (ax - bx).abs() * 2 / (brib + bria);
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn score<P: Pixel<Subpixel = u8> + 'static>(im: &ImageBuffer<P, Vec<u8>>) -> u32 {
    let mut sc = 0;
    for a in im.pixels() {
        sc += if a.to_luma().data[0] > 64 { 1 } else { 0 };
    }
    return sc;
}

fn binarize<P: Pixel<Subpixel = u8> + 'static>(im: &ImageBuffer<P, Vec<u8>>) -> GrayImage {
    let dim = im.dimensions();
    let mut dst = Vec::with_capacity((dim.0 * dim.1) as usize);
    for a in im.pixels() {
        dst.push(if a.to_luma().data[0] > 64 { 255 } else { 0 });
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn derivative<P>(im: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let kernel: &[f32] = &[0.0, -1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0];
    filter3x3(im, kernel)
}

fn to_buffer<R: Read>(img: JPEGDecoder<R>) -> GrayImage {
    let dim = img.dimensions();
    let typ = img.colortype();
    let vec = img.read_image().unwrap();

    return match typ {
        ColorType::Gray(_) => ImageBuffer::from_vec(dim.0 as u32, dim.1 as u32, vec).unwrap(),
        ColorType::RGB(_) => {
            ImageBuffer::<Rgb<u8>, Vec<u8>>::from_vec(dim.0 as u32, dim.1 as u32, vec)
                .unwrap()
                .convert()
        }
        _ => panic!("unsupported color type"),
    };
}

fn from_yuyv_vec(data: Vec<u8>) -> Option<GrayImage> {
    return ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(
        1280,
        720, // size is temporary fixed as its camera setting
        data.iter()
            .step_by(2) // skip u and v data
            .cloned()
            .collect(),
    );
}

fn prepare(name: &String) -> ImageResult<JPEGDecoder<BufReader<std::fs::File>>> {
    //println!("name={}", name);
    return match fs::File::open(name) {
        Ok(f) => {
            let mut reader = BufReader::new(f);
            JPEGDecoder::new(reader)
        }
        Err(err) => Err(image::ImageError::IoError(err)),
    };
}

enum ImageIter {
    CameraIter(Camera),
    ImageDirIter(fs::ReadDir),
}

impl Iterator for ImageIter {
    type Item = GrayImage;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ImageIter::CameraIter(cam) => {
                let frame = cam.capture().unwrap();
                from_yuyv_vec(frame[..].to_vec())
            }
            ImageIter::ImageDirIter(dir) => match dir.next() {
                Some(entry) => match prepare(
                    &entry
                        .map(|e| e.path().as_path().to_str().unwrap().to_string())
                        .unwrap(),
                ) {
                    Ok(image) => Some(to_buffer(image)),
                    Err(e) => {
                        eprintln!("dirIter failed: error={}", e);
                        None
                    }
                },
                None => None,
            },
        }
    }
}

fn start_camera(dev: &str) -> ImageIter {
    let mut camera = match Camera::new(dev) {
        Ok(c) => c,
        Err(e) => panic! {format!("failed to open camera: {}", e)},
    };
    camera
        .start(&Config {
            interval: (1, 10),
            resolution: (1280, 720),
            format: b"YUYV",
            ..Default::default()
        })
        .expect("failed to start camera");
    for feat in camera.formats() {
        let info = feat.unwrap();
        println!("failed to start camera: {:?}: {}", info.format, info.description);
    }
    return ImageIter::CameraIter(camera);
}

struct Uploader {
    client: reqwest::Client,
    server: String,
    user: Option<String>,
    pass: Option<String>,
}

#[derive(Debug)]
enum UploaderError {
    FileIOError(std::io::Error),
    ReqwestError(reqwest::Error),
}

fn do_post(info: &mut Option<Uploader>, name: &String) -> Result<(), UploaderError> {
    match info {
        Some(inf) => fs::File::open(name)
            .map_err(|e| UploaderError::FileIOError(e))
            .and_then(|file| {
                match (inf.user.clone(), inf.pass.clone()) {
                    (Some(u), p) => inf
                        .client
                        .post(&inf.server)
                        .query(&[("name", name)])
                        .basic_auth(u, p)
                        .body(file)
                        .send(),
                    (None, _) => inf
                        .client
                        .post(&inf.server)
                        .query(&[("name", name)])
                        .body(file)
                        .send(),
                }
                .map_err(|e| UploaderError::ReqwestError(e))
            })
            .and_then(|_| fs::remove_file(name).map_err(|e| UploaderError::FileIOError(e)))
            .map(|_| ()),
        None => Ok(()),
    }
}

fn main() {
    let matches = App::new("small recorder")
        .version("0.0.1")
        .arg(
            Arg::with_name("device")
                .long("dev")
                .takes_value(true)
                .default_value("/dev/video0")
                .help("device name"),
        )
        .arg(Arg::with_name("forever").short("f").help("run forever"))
        .arg(
            Arg::with_name("dir")
                .short("i")
                .long("image_dir")
                .takes_value(true)
                .help("image directory used instead of camera"),
        )
        .arg(
            Arg::with_name("rotation")
                .short("r")
                .long("record_rotation")
                .takes_value(true)
                .default_value("3000")
                .help("how many pics to take before rotate its numbering"),
        )
        .arg(
            Arg::with_name("start_from")
                .short("s")
                .long("start-count")
                .takes_value(true)
                .default_value("0")
                .help("the start number to count the saved frame"),
        )
        .arg(
            Arg::with_name("dst")
                .short("d")
                .long("dst")
                .takes_value(true)
                .default_value("movie")
                .help("destination directory"),
        )
        .arg(
            Arg::with_name("gif")
                .short("g")
                .long("gif")
                .help("gif mode"),
        )
        .arg(
            Arg::with_name("jpeg")
                .short("j")
                .long("jpeg")
                .help("jpeg mode"),
        )
        .arg(
            Arg::with_name("post")
                .short("p")
                .long("post")
                .takes_value(true)
                .help("post the data"),
        )
        .arg(
            Arg::with_name("basic-username")
                .long("basic-username")
                .takes_value(true)
                .help("the username for basic authentication"),
        )
        .arg(
            Arg::with_name("basic-password")
                .long("basic-password")
                .takes_value(true)
                .help("the password for basic authentication"),
        )
        .get_matches();

    let mut iter = if matches.is_present("dir") {
        ImageIter::ImageDirIter(fs::read_dir(Path::new(matches.value_of("dir").unwrap())).unwrap())
    } else {
        start_camera(matches.value_of("device").unwrap())
    };

    let rot: u32 = u32::from_str_radix(matches.value_of("rotation").unwrap(), 10).unwrap();
    let start: u32 = u32::from_str_radix(matches.value_of("start_from").unwrap(), 10).unwrap();
    let dst = matches.value_of("dst").unwrap().to_string();
    let gif = matches.is_present("gif");
    let jpg = !gif || matches.is_present("jpeg");

    let mut upload_info = match (
        matches.value_of("post"),
        matches.value_of("basic-username"),
        matches.value_of("basic-password"),
    ) {
        (Some(server), u, p) => Some(Uploader {
            client: reqwest::Client::new(),
            server: server.to_string(),
            user: u.map(|x| x.to_string()),
            pass: p.map(|x| x.to_string()),
        }),
        _ => None,
    };

    let (sender, receiver): (
        mpsc::Sender<Option<Option<(u32, Arc<GrayImage>)>>>,
        mpsc::Receiver<Option<Option<(u32, Arc<GrayImage>)>>>,
    ) = mpsc::channel();

    // palette for gif; map the value to monochrome gradation palette
    let palette = if gif {
        let mut init = Vec::<u8>::with_capacity(16 * 3);
        for i in 0..15 {
            init.push(i * 16); // r
            init.push(i * 16); // g
            init.push(i * 16); // b
        }
        init
    } else {
        Vec::new()
    };

    let saver_thread = thread::spawn(move || {
        let mut enc: Option<gif::Encoder<BufWriter<fs::File>>> = None;
        let mut g_name = "".to_string();
        loop {
            match receiver.recv().unwrap() {
                Some(Some((name, data))) => {
                    if gif {
                        if enc.is_none() {
                            g_name = format!("{}/animation-{:>06}.gif", dst, name);
                            enc = Some(gif::Encoder::new(BufWriter::new(
                                fs::File::create(&g_name).unwrap(),
                            )));
                        }
                        let frame = gif::Frame::from_palette_pixels(
                            1280,
                            720,
                            &reduce_color(&data, 4),
                            &palette,
                            None,
                        );
                        let mut encoder = enc.unwrap();
                        encoder.encode(&frame).unwrap();
                        enc = Some(encoder);
                    }
                    if jpg {
                        let p_name = format!("{}/picture-{:06}.jpg", &dst, name);
                        data.save(&p_name).unwrap();
                        match do_post(&mut upload_info, &p_name) {
                            Ok(()) => {}
                            Err(e) => eprintln!("do_post failed(jpg): {:?}", e),
                        }
                    }
                }
                Some(None) => {
                    enc = None;
                    match do_post(&mut upload_info, &g_name) {
                        Ok(()) => {}
                        Err(e) => eprintln!("do_post failed(gif): {:?}", e),
                    }
                }
                None => {
                    break;
                }
            }
        }
    });

    struct ProcessingContext<T> {
        index: u32,
        prev: Arc<T>,
        diff: Arc<T>,
        avg_ms: f32,
        start_time: SystemTime,
        blank_frame_deadline: u32,
    }

    // temporal first diff image
    let black: GrayImage = ImageBuffer::from_pixel(1280, 720, Luma { data: [0] });

    let init_context = ProcessingContext::<GrayImage> {
        index: start,
        prev: Arc::new(iter.next().unwrap()),
        diff: Arc::new(black),
        avg_ms: 1.0,
        start_time: SystemTime::now(),
        blank_frame_deadline: 0,
    };

    iter.map(|x| Arc::new(x))
        .fold(init_context, |context, current_img| {
            let d = binarize(&diff(&context.prev, &current_img));
            let buf = and(&d, &context.diff);
            let sc = score(&buf);
            let sent = if sc > 100 {
                let sent_n = Arc::clone(&current_img);
                sender.send(Some(Some((context.index, sent_n))));
                true
            } else if context.blank_frame_deadline == 1 {
                sender.send(Some(None));
                false
            } else {
                false
            };
            let t = context.avg_ms * 0.8
                + context
                    .start_time
                    .elapsed()
                    .map(|x| x.as_millis() as f32)
                    .unwrap_or(1000.0)
                    * 0.2;

            println!(
                "image {:>08}, avg_time: {:>7.3}ms, score: {:>10}",
                context.index, t, sc
            );
            ProcessingContext::<GrayImage> {
                index: if sent {
                    (context.index + 1) % rot
                } else {
                    context.index
                },
                prev: current_img,
                diff: Arc::new(d),
                avg_ms: t,
                start_time: SystemTime::now(),
                blank_frame_deadline: if sent {
                    /* reset deadline approx. 7s */
                    70
                } else if context.blank_frame_deadline == 0 {
                    0
                } else {
                    context.blank_frame_deadline - 1
                },
            }
        });

    println!("Capturing ended. Finishing...");
    sender.send(None).unwrap();

    saver_thread.join().unwrap();
}
