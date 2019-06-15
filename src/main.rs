extern crate image;

use std::io::Read;
use std::env::args;
use std::fs::File;
use image::{Pixel, ImageBuffer, Rgb, RgbImage, ImageDecoder, ImageResult};
use image::jpeg::{JPEGDecoder};

fn compare(ima: RgbImage, imb: RgbImage) -> RgbImage {
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();
    let suma: u64 = ima.iter().fold(0 as u64, |b, s| b + *s as u64);
    let sumb: u64 = imb.iter().fold(0 as u64, |b, s| b + *s as u64);

    let bufa = ima.into_raw();
    let bufb = imb.into_raw();
    for (a, b) in bufa.iter().zip(&bufb) {
        let ax = *a as u64 * sumb;
        let bx = *b as u64 * suma;
        let p = if ax > bx {ax -bx} else {bx - ax} * 2 / (suma + sumb);
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
}

fn to_buffer<P, R>(img: JPEGDecoder<R>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel=u8> + 'static, R: Read {
    let dim = img.dimensions();
    let vec = img.read_image_with_progress(
        |p| { println!("{:?}", p) }
        ).unwrap();
    println!("size in read: {}kB. dim0 x dim1 = {} x {} = {}",
             vec.len() / 8000, dim.0, dim.1, dim.0 * dim.1);
    let buf = ImageBuffer::from_vec(dim.0 as u32, dim.1 as u32, vec).expect("test2");
    return buf;
}

fn prepare(name: &String) -> ImageResult<JPEGDecoder<std::fs::File>> {
    return match File::open(name) {
        Ok(f) => JPEGDecoder::new(f),
        Err(err) => Err(image::ImageError::IoError(err))
    };
}

fn main() {
    let mut arg: Vec<String> = args().collect();
    let mut file_a = &"snap.jpg".to_string();
    let mut file_b = &"snap1.jpg".to_string();
    if arg.len() > 2 {
        file_a = &arg[1];
        file_b = &arg[2];
    }
    println!(":test: {} - {}", file_a, file_b);

    let buf = compare(to_buffer(prepare(file_a).unwrap()), to_buffer(prepare(file_b).unwrap()));
    buf.save("subst.jpg").unwrap();
}
