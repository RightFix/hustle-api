use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

use crate::{
    error::{AppError, AppResult},
};

const MAX_RAW_BYTES: usize = 10 * 1024 * 1024;
const JPEG_QUALITY: u8 = 85;

/// Normalized image ready to be served: EXIF-oriented, scaled to fit,
/// flattened onto white, and re-encoded as JPEG.
pub struct JpegImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Validate, orient, scale and flatten a JPEG, PNG or WebP image.
///
/// Returns the re-encoded bytes plus the resulting dimensions. The raw
/// buffer may be at most 10 MB; `max_dimension` controls the resize limit.
pub fn process_jpeg_image(raw: &[u8], max_dimension: u32) -> AppResult<JpegImage> {
    if raw.len() > MAX_RAW_BYTES {
        return Err(AppError::PayloadTooLarge(
            "Images must be 10MB or smaller".into(),
        ));
    }
    let format = image::guess_format(raw).map_err(|_| {
        AppError::Unprocessable("Only JPEG, PNG, or WebP images are supported".into())
    })?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP) {
        return Err(AppError::Unprocessable(
            "Only JPEG, PNG, or WebP images are supported".into(),
        ));
    }
    let mut img = image::load_from_memory(raw)
        .map_err(|_| AppError::Unprocessable("Could not read image".into()))?;

    apply_exif_orientation(&mut img, raw);

    if img.width() > max_dimension || img.height() > max_dimension {
        img = img.resize(max_dimension, max_dimension, FilterType::Triangle);
    }

    // Flatten any transparency onto white and re-encode as JPEG.
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        rgb.put_pixel(
            x,
            y,
            image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]),
        );
    }
    let flat = DynamicImage::ImageRgb8(rgb);
    let (width, height) = (flat.width(), flat.height());

    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&flat)
        .map_err(|_| AppError::Internal)?;

    Ok(JpegImage {
        bytes: out,
        width,
        height,
    })
}

fn apply_exif_orientation(img: &mut DynamicImage, raw: &[u8]) {
    let orientation = exif::Reader::new()
        .read_from_container(&mut Cursor::new(raw))
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        });
    if let Some(orientation) = orientation {
        if let Some(o) = image::metadata::Orientation::from_exif(orientation as u8) {
            img.apply_orientation(o);
        }
    }
}
