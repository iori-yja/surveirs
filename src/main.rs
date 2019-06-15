extern crate image;

use std::io::Read;
use std::env::args;
use std::fs::File;
use image::{Pixel, ImageBuffer, Rgb, GrayImage, RgbImage, ImageDecoder, ImageResult, ConvertBuffer};
use image::jpeg::{JPEGDecoder};
use image::imageops::filter3x3;

fn and<P>(ima: ImageBuffer<P, Vec<u8>>, imb: ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel = u8> + 'static {
    let mut dst = Vec::with_capacity(ima.len());
    let dim = ima.dimensions();

    let bufa = ima.into_raw();
    let bufb = imb.into_raw();
    for (a, b) in bufa.iter().zip(&bufb) {
        let (ax, bx) = (*a as i64, *b as i64);
        let p = if (ax - bx).abs() < 50 {*a} else {0};
        dst.push(p as u8);
    }
    return ImageBuffer::from_vec(dim.0, dim.1, dst).expect("test");
}

fn diff<P>(ima: ImageBuffer<P, Vec<u8>>, imb: ImageBuffer<P, Vec<u8>>) -> ImageBuffer<P, Vec<u8>>
            where P: Pixel<Subpixel = u8> + 'static {
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
    let kernel: &[f32] = &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    //let kernel: &[f32] = &[0.0,-1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0];
    filter3x3(im, kernel)
}

//fn to_buffer<P, R>(img: JPEGDecoder<R>) -> ImageBuffer<P, Vec<u8>>
//            where P: Pixel<Subpixel=u8> + 'static, R: Read {
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
    return match File::open(name) {
        Ok(f) => JPEGDecoder::new(f),
        Err(err) => Err(image::ImageError::IoError(err))
    };
}

fn main() {
    let mut arg: Vec<String> = args().collect();
    let mut file_a = &"move/snap-s.jpg".to_string();
    let mut file_b = &"move/snap-t.jpg".to_string();
    let mut file_c = &"move/snap-u.jpg".to_string();
    if arg.len() > 2 {
        file_a = &arg[1];
        file_b = &arg[2];
        file_c = &arg[3];
    }

    let buf1 = diff(to_buffer(prepare(file_a).unwrap()),
                      to_buffer(prepare(file_b).unwrap()));
    let buf2 = diff(to_buffer(prepare(file_b).unwrap()),
                      to_buffer(prepare(file_c).unwrap()));
    buf1.save("move/subst1.jpg").unwrap();
    buf2.save("move/subst2.jpg").unwrap();
    let buf3 = and(binarize(&buf1), binarize(&buf2));
    buf3.save("move/subst3.jpg").unwrap();
}
