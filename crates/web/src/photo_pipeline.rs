use bytes::Bytes;

#[derive(Debug, thiserror::Error)]
pub enum PhotoError {
    #[error("could not decode image: {0}")]
    Decode(String),
    #[error("could not encode image: {0}")]
    Encode(String),
    #[error("upload too large: {got} bytes (max {max})")]
    TooLarge { got: usize, max: usize },
}

/// A photo after the pipeline: bounded dimensions, JPEG, and — because we
/// fully decode and re-encode — provably free of the original's metadata.
/// Phone EXIF carries GPS coordinates; stripping it is a privacy requirement
/// (duck-voyage.md §7), not an optimization.
pub struct ProcessedPhoto {
    pub bytes: Bytes,
    pub content_type: &'static str,
}

pub struct PhotoPipeline;

impl PhotoPipeline {
    pub const MAX_UPLOAD_BYTES: usize = 12 * 1024 * 1024;
    pub const MAX_DIMENSION: u32 = 1600;

    pub fn process(input: &[u8]) -> Result<ProcessedPhoto, PhotoError> {
        (input.len() <= Self::MAX_UPLOAD_BYTES)
            .then_some(())
            .ok_or(PhotoError::TooLarge { got: input.len(), max: Self::MAX_UPLOAD_BYTES })?;
        let decoded =
            image::load_from_memory(input).map_err(|e| PhotoError::Decode(e.to_string()))?;
        let oversized =
            decoded.width() > Self::MAX_DIMENSION || decoded.height() > Self::MAX_DIMENSION;
        let bounded = if oversized {
            decoded.resize(
                Self::MAX_DIMENSION,
                Self::MAX_DIMENSION,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            decoded
        };
        let mut out = Vec::new();
        // Re-encoding from decoded pixels: EXIF/XMP/GPS from the upload
        // cannot survive because only pixel data crosses this boundary.
        bounded
            .into_rgb8()
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg)
            .map_err(|e| PhotoError::Encode(e.to_string()))?;
        Ok(ProcessedPhoto { bytes: Bytes::from(out), content_type: "image/jpeg" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn sample_jpeg() -> Vec<u8> {
        let img = ImageBuffer::from_fn(64, 48, |x, y| Rgb([x as u8, y as u8, 128u8]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .unwrap();
        bytes
    }

    /// A JPEG with an EXIF APP1 segment spliced in directly after SOI.
    fn jpeg_with_exif() -> Vec<u8> {
        let plain = sample_jpeg();
        let exif_body: &[u8] = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0GPS-LATITUDE-HERE";
        let mut tainted = plain[..2].to_vec(); // SOI
        tainted.extend([0xFF, 0xE1]); // APP1 marker
        tainted.extend(u16::try_from(exif_body.len() + 2).unwrap().to_be_bytes());
        tainted.extend(exif_body);
        tainted.extend(&plain[2..]);
        tainted
    }

    #[test]
    fn strips_exif_and_reencodes() {
        let tainted = jpeg_with_exif();
        assert!(tainted.windows(4).any(|w| w == b"Exif"), "fixture must contain EXIF");
        let processed = PhotoPipeline::process(&tainted).unwrap();
        assert_eq!(processed.content_type, "image/jpeg");
        assert!(
            !processed.bytes.windows(4).any(|w| w == b"Exif"),
            "EXIF survived the pipeline"
        );
        assert!(!processed.bytes.windows(12).any(|w| w == b"GPS-LATITUDE"));
    }

    #[test]
    fn bounds_dimensions() {
        let big = {
            let img = ImageBuffer::from_pixel(2400, 1200, Rgb([10u8, 20, 30]));
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
                .unwrap();
            bytes
        };
        let processed = PhotoPipeline::process(&big).unwrap();
        let reloaded = image::load_from_memory(&processed.bytes).unwrap();
        assert!(reloaded.width() <= PhotoPipeline::MAX_DIMENSION);
        assert!(reloaded.height() <= PhotoPipeline::MAX_DIMENSION);
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            PhotoPipeline::process(b"not an image at all"),
            Err(PhotoError::Decode(_))
        ));
    }
}
