#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CoordinatesError {
    #[error("latitude {0} outside -90..=90")]
    Latitude(f64),
    #[error("longitude {0} outside -180..=180")]
    Longitude(f64),
}

/// A WGS84 point captured from the finder's browser. Range-checked on
/// construction; both components always travel together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coordinates {
    latitude: f64,
    longitude: f64,
}

impl Coordinates {
    pub fn new(latitude: f64, longitude: f64) -> Result<Self, CoordinatesError> {
        ((-90.0..=90.0).contains(&latitude) && latitude.is_finite())
            .then_some(())
            .ok_or(CoordinatesError::Latitude(latitude))?;
        ((-180.0..=180.0).contains(&longitude) && longitude.is_finite())
            .then_some(())
            .ok_or(CoordinatesError::Longitude(longitude))?;
        Ok(Self { latitude, longitude })
    }

    pub fn latitude(self) -> f64 {
        self.latitude
    }

    pub fn longitude(self) -> f64 {
        self.longitude
    }
}

impl std::fmt::Display for Coordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.5}, {:.5}", self.latitude, self.longitude)
    }
}
