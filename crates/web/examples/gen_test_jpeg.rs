//! Dev utility: write a JPEG with an EXIF APP1 segment (fake GPS payload)
//! for exercising the EXIF-stripping pipeline end-to-end.
//! Usage: cargo run -p web --example gen_test_jpeg -- /path/out.jpg

fn main() {
    let path = std::env::args().nth(1).expect("usage: gen_test_jpeg <out.jpg>");
    let img = image::ImageBuffer::from_fn(320, 240, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    let mut plain = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut plain), image::ImageFormat::Jpeg)
        .expect("encode");
    let exif_body: &[u8] = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0GPS-LATITUDE-SECRET-42.1234";
    let mut tainted = plain[..2].to_vec();
    tainted.extend([0xFF, 0xE1]);
    tainted.extend(u16::try_from(exif_body.len() + 2).unwrap().to_be_bytes());
    tainted.extend(exif_body);
    tainted.extend(&plain[2..]);
    std::fs::write(&path, &tainted).expect("write");
    eprintln!("wrote {path} ({} bytes, EXIF embedded)", tainted.len());
}
