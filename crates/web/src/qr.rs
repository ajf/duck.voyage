use domain::DuckCode;
use qrcode::render::Renderer;
use qrcode::{EcLevel, QrCode};

#[derive(Debug, thiserror::Error)]
pub enum QrError {
    #[error("QR encoding failed: {0}")]
    Encode(String),
    #[error("PNG encoding failed: {0}")]
    Png(String),
}

/// QR label rendering (duck-voyage.md §9): the full URL in ALL CAPS keeps the
/// payload in QR alphanumeric mode; ECC level Q tolerates 25% damage. With
/// the canonical 29-char `HTTPS://DUCK.VOYAGE/D/XXXXXXX` payload this yields
/// a Version 2 (25×25) code.
pub struct QrLabel;

impl QrLabel {
    /// The ALL-CAPS scan URL for a duck.
    pub fn url(base_url: &str, code: &DuckCode) -> String {
        format!("{}/D/{}", base_url.to_uppercase(), code.as_str())
    }

    /// Render the duck's QR as a PNG (one module = 8px, quiet zone included).
    pub fn png(base_url: &str, code: &DuckCode) -> Result<Vec<u8>, QrError> {
        let qr = QrCode::with_error_correction_level(Self::url(base_url, code), EcLevel::Q)
            .map_err(|e| QrError::Encode(e.to_string()))?;
        let image = Renderer::<image::Luma<u8>>::new(&qr.to_colors(), qr.width(), 4)
            .module_dimensions(8, 8)
            .build();
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| QrError::Png(e.to_string()))?;
        Ok(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrcode::Version;

    /// The production payload must stay within Version 2 at ECC-Q — 29
    /// alphanumeric chars is exactly the capacity, with the doc's canonical
    /// domain. (Local dev URLs are longer and may bump the version; that's
    /// fine, only printed labels care.)
    #[test]
    fn production_url_fits_version_2_q() {
        let code = DuckCode::parse("QK7XFRZ").unwrap();
        let url = QrLabel::url("https://duck.voyage", &code);
        assert_eq!(url, "HTTPS://DUCK.VOYAGE/D/QK7XFRZ");
        assert_eq!(url.len(), 29);
        let qr = QrCode::with_error_correction_level(&url, EcLevel::Q).unwrap();
        assert_eq!(qr.version(), Version::Normal(2));
    }

    #[test]
    fn png_renders() {
        let code = DuckCode::parse("QK7XFRZ").unwrap();
        let png = QrLabel::png("https://duck.voyage", &code).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
