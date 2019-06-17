extern crate clap;
extern crate image;
extern crate rscam;

use clap::{App, Arg};
use image::imageops::filter3x3;
use image::jpeg::JPEGDecoder;
use image::{
    ColorType, ConvertBuffer, GrayImage, ImageBuffer, ImageDecoder, ImageResult, Luma, Pixel, Rgb,
};
use rscam::{Camera, Config};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

fn and<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();

    let bufa = ima.pixels();
    let bufb = imb.pixels();
    for (a, b) in bufa.zip(bufb) {
        let ax = a.to_luma().data[0] as i64;
        let bx = b.to_luma().data[0] as i64;
        let p = if (ax - bx).abs() < 50 { ax } else { 0 };
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
}

fn brightness<P>(im: &ImageBuffer<P, Vec<u8>>) -> i64
where
    P: Pixel<Subpixel = u8> + 'static,
{
    return im
        .pixels()
        .fold(0 as i64, |b, p| b + p.to_luma().data[0] as i64);
}

fn diff<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();
    let bria = brightness(ima);
    let brib = brightness(imb);

    let bufa = ima.pixels();
    let bufb = imb.pixels();
    for (a, b) in bufa.zip(bufb) {
        let ax = a.to_luma().data[0] as i64 * brib;
        let bx = b.to_luma().data[0] as i64 * bria;
        let p = if ax > bx { ax - bx } else { bx - ax } * 2 / (brib + bria);
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn score<P: Pixel<Subpixel = u8> + 'static>(im: &ImageBuffer<P, Vec<u8>>) -> u64 {
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
    let vec = img.read_image(
        //|p| { println!("{:?}", p) }
        ).unwrap();
    //println!("size in read: {}kB. dim0 x dim1 = {} x {} = {}",
    //         vec.len() / 1000, dim.0, dim.1, dim.0 * dim.1);
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

fn from_yuyv_vec(data: Vec<u8>) -> GrayImage {
    return ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(
        1280,
        720, // size is temporary fixed as its camera setting
        data.iter()
            .step_by(2) // skip u and v data
            .cloned()
            .collect(),
    )
    .unwrap();
}

fn prepare(name: &String) -> ImageResult<JPEGDecoder<std::fs::File>> {
    //println!("name={}", name);
    return match fs::File::open(name) {
        Ok(f) => JPEGDecoder::new(f),
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
                Some(from_yuyv_vec(frame[..].to_vec()))
            }
            ImageIter::ImageDirIter(dir) => match dir.next() {
                Some(entry) => match prepare(
                    &entry
                        .map(|e| e.path().as_path().to_str().unwrap().to_string())
                        .unwrap(),
                ) {
                    Ok(image) => Some(to_buffer(image)),
                    Err(e) => {
                        println!("error={}", e);
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
        println!("{:?}: {}", info.format, info.description);
    }
    return ImageIter::CameraIter(camera);
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
        .get_matches();

    let mut iter = if matches.is_present("dir") {
        ImageIter::ImageDirIter(fs::read_dir(Path::new(matches.value_of("dir").unwrap())).unwrap())
    } else {
        start_camera(matches.value_of("device").unwrap())
    };

    let rot: u32 = u32::from_str_radix(matches.value_of("rotation").unwrap(), 10).unwrap();
    let start: u32 = u32::from_str_radix(matches.value_of("start_from").unwrap(), 10).unwrap();

    // temporal first diff image
    let black: GrayImage = ImageBuffer::from_pixel(1280, 720, Luma { data: [0] });

    // first frame
    let f = iter.next().unwrap();

    let (sender, receiver): (
        mpsc::Sender<Option<(String, Arc<GrayImage>)>>,
        mpsc::Receiver<Option<(String, Arc<GrayImage>)>>,
    ) = mpsc::channel();
    let saver_thread = thread::spawn(move || {
        while true {
            match receiver.recv().unwrap() {
                Some((name, data)) => {
                    data.save(name);
                }
                None => {
                    break;
                }
            };
        }
    });

    iter.map(|x| Arc::new(x)).fold(
        (start, Arc::new(f), Arc::new(black)),
        |(i, prev, dif), n| {
            let d = binarize(&diff(&prev, &n));
            let buf = and(&d, &dif);
            let sc = score(&buf);
            let mut j = i;
            if sc > 100 {
                let sent_n = Arc::clone(&n);
                sender
                    .send(Some((format!("movie/diff-{:>08}.jpg", i), Arc::new(buf))))
                    .unwrap();
                sender
                    .send(Some((format!("movie/frame-{:>08}.jpg", i), sent_n)))
                    .unwrap();
                j = i + 1;
                if j > rot {
                    j = 0;
                }
            }
            println!("image {:>08}, score: {}", i, sc);
            (j, n, Arc::new(d))
        },
    );

    saver_thread.join().unwrap();
}
