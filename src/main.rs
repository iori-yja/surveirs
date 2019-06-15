extern crate image;
extern crate rscam;
extern crate chrono;

use std::io::{Read, Write};
use std::env::args;
use std::thread;
use std::fs;
use chrono::prelude::*;
use rscam::{Camera, Config, FormatInfo, FormatIter};
use image::{Pixel, ImageBuffer, Rgb, GrayImage, RgbImage, ImageDecoder, ImageResult, ConvertBuffer};
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
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
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
    let buf: RgbImage = ImageBuffer::from_vec(dim.0 as u32, dim.1 as u32, vec).expect("test2");
    return buf.convert();
}

fn prepare(name: &String) -> ImageResult<JPEGDecoder<std::fs::File>> {
    return match fs::File::open(name) {
        Ok(f) => JPEGDecoder::new(f),
        Err(err) => Err(image::ImageError::IoError(err))
    };
}

fn main() {
    let mut arg: Vec<String> = args().collect();
    let mut camera = if arg.len() > 2 {
        Camera::new(&arg[1]).unwrap()
    } else {
        Camera::new("/dev/video0").unwrap()
    };

    camera.start(&Config {
        interval: (1, 10),
        resolution: (1280, 720),
        format: b"RGB3",
        ..Default::default()
    }).unwrap();

    let mut frames: Vec<GrayImage> = Vec::with_capacity(10);
    let mut adiff: GrayImage;
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
    adiff = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_vec(1280, 720, frame[..].to_vec()).unwrap().convert();
    for i in 0..1000 {
        let mut sc = 0;
        let now = Utc::now();
        let frame = camera.capture().unwrap();
        let buf: GrayImage = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_vec(1280, 720, frame[..].to_vec()).unwrap().convert();
        frames.push(buf);
        if i == 1 {
            adiff = binarize(&diff(&frames[i-1], &frames[i]));
        }
        if i > 1 {
            let bdiff = binarize(&diff(&frames[i-1], &frames[i]));
            let buf = and(&bdiff, &adiff);
            sc = score(&buf);
            buf.save(format!("movie/diff-{}.jpg", i));
            adiff = bdiff;
        }
        if sc > 100 {
            frames[i].save(&format!("movie/frame-{}.jpg", i)).unwrap();
        }
        println!("image {}, took {}, score: {}", i, Utc::now() - now, sc);
    }
}
