extern crate image;
extern crate rscam;
extern crate chrono;
extern crate clap;

use std::io::Read;
use std::env;
use std::fs;
use clap::{Arg, App};
use chrono::prelude::*;
use rscam::{Camera, Config};
use image::{Pixel, ImageBuffer, Luma, GrayImage, RgbImage, ImageDecoder, ImageResult, ConvertBuffer};
use image::jpeg::{JPEGDecoder};
use image::imageops::filter3x3;


fn and<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel = u8> + 'static {
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();

    let bufa = ima.pixels();
    let bufb = imb.pixels();
    for (a, b) in bufa.zip(bufb) {
        let ax = a.to_luma().data[0] as i64;
        let bx = b.to_luma().data[0] as i64;
        let p = if (ax - bx).abs() < 50 {ax} else {0};
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
}

fn brightness<P>(im: &ImageBuffer<P, Vec<u8>>) -> i64
            where P: Pixel<Subpixel = u8> + 'static {
    return im.pixels().fold(0 as i64, |b, p| b + p.to_luma().data[0] as i64);
}

fn diff<P>(ima: &ImageBuffer<P, Vec<u8>>, imb: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel = u8> + 'static {
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();
    let bria = brightness(ima);
    let brib = brightness(imb);

    let bufa = ima.pixels();
    let bufb = imb.pixels();
    for (a, b) in bufa.zip(bufb) {
        let ax = a.to_luma().data[0] as i64 * brib;
        let bx = b.to_luma().data[0] as i64 * bria;
        let p = if ax > bx {ax -bx} else {bx - ax} * 2 / (brib + bria);
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn score<P: Pixel<Subpixel = u8> + 'static>(im: &ImageBuffer<P, Vec<u8>>) -> u64 {
    let mut sc = 0;
    for a in im.pixels() {
        sc += if a.to_luma().data[0] > 64 {1} else {0};
    }
    return sc;
}

fn binarize<P: Pixel<Subpixel = u8> + 'static>(im: &ImageBuffer<P, Vec<u8>>) -> GrayImage {
    let dim = im.dimensions();
    let mut dst = Vec::with_capacity((dim.0 * dim.1) as usize);
    for a in im.pixels() {
        dst.push(if a.to_luma().data[0] > 64 {255} else {0});
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).unwrap();
}

fn derivative<P>(im: &ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel = u8> + 'static {
    let kernel: &[f32] = &[0.0,-1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0];
    filter3x3(im, kernel)
}

fn to_buffer<R: Read>(img: JPEGDecoder<R>) -> GrayImage {
    let dim = img.dimensions();
    let vec = img.read_image_with_progress(
        |p| { println!("{:?}", p) }
        ).unwrap();
    println!("size in read: {}kB. dim0 x dim1 = {} x {} = {}",
             vec.len() / 1000, dim.0, dim.1, dim.0 * dim.1);
    let buf: RgbImage = ImageBuffer::from_vec(dim.0 as u32, dim.1 as u32, vec).unwrap();
    return buf.convert();
}

fn from_yuyv_vec(data: Vec<u8>) -> GrayImage {
    return ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(
        1280, 720, // size is temporary fixed as its camera setting
        data.iter()
        .step_by(2) // skip u and v data
        .cloned().collect()
    ).unwrap();
}

fn prepare(name: &String) -> ImageResult<JPEGDecoder<std::fs::File>> {
    return match fs::File::open(name) {
        Ok(f) => JPEGDecoder::new(f),
        Err(err) => Err(image::ImageError::IoError(err))
    };
}

fn main() {
    let matches = App::new("small recorder")
        .version("0.0.1")
        .arg(Arg::with_name("device")
             .long("dev")
             .takes_value(true)
             .default_value("/dev/video0")
             .help("device name"))
        .arg(Arg::with_name("forever")
             .short("f")
             .help("run forever"))
        .get_matches();

    let mut camera = match Camera::new(matches.value_of("device").unwrap()) {
        Ok(c) => c,
        Err(e) => panic!{format!("failed to open camera: {}", e)}
    };
    camera.start(&Config {
        interval: (1, 10),
        resolution: (1280, 720),
        format: b"YUYV",
        ..Default::default()
    }).expect("failed to start camera");
    for feat in camera.formats() {
        let info = feat.unwrap();
        println!("{:?}: {}", info.format, info.description);
    }

    let mut frames: Vec<GrayImage> = Vec::with_capacity(10);
    /*
    for i in 0..10 {
        frames.push(to_buffer(prepare(&format!("frame-{}.jpg", i)).unwrap()));
    }
    for i in 1..9 {
        let buf1 = diff(&frames[i-1], &frames[i]);
        let buf2 = diff(&frames[i], &frames[i+1]);
        let buf = and(&buf1, &buf2);
        buf.save(format!("diff-{}.jpg", i)).unwrap();
    }
    */


    // first buffer comes slowly, so stash it.
    let frame = camera.capture().unwrap();
    // To be filled in i == 1 condition before use
    let mut adiff: GrayImage = ImageBuffer::new(0,0);

    let mut i = 0;
    while i < 1000 || matches.is_present("forever") {
        let mut sc = 0;
        let now = Utc::now();
        let frame = camera.capture().unwrap();
        frames.push(from_yuyv_vec(frame[..].to_vec()));
        if i == 1 {
            adiff = binarize(&diff(&frames[i-1], &frames[i]));
        } else if i > 1 {
            let bdiff = binarize(&diff(&frames[i-1], &frames[i]));
            let buf = and(&bdiff, &adiff);
            sc = score(&buf);
            buf.save(format!("movie/diff-{:>08}.jpg", i)).unwrap();
            adiff = bdiff;
        }
        if sc > 100 {
            frames[i].save(&format!("movie/frame-{:>08}.jpg", i)).unwrap();
        }
        println!("image {:>08}, took {}, score: {}", i, Utc::now() - now, sc);
        i += 1;
    }
}
