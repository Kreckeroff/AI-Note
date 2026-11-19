use std::io::Cursor;

use image::{ImageDecoder, ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder};
use tokio::sync::Semaphore;

pub(crate) const MAX_COVER_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const COVER_IMAGE_CONTENT_TYPE: &str = "image/jpeg";
const COVER_WIDTH: u32 = 1000;
const COVER_HEIGHT: u32 = 340;
const MAX_IMAGE_SIDE: u32 = 8192;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_DECODED_BYTES: u64 = MAX_IMAGE_PIXELS * 8;
static DECODERS: Semaphore = Semaphore::const_new(2);

#[derive(Debug)]
pub(crate) struct CoverImage(Vec<u8>);

impl CoverImage {
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NormalizeCoverError {
    Invalid,
    Busy,
}

pub(crate) async fn normalize_cover_image(
    bytes: Vec<u8>,
) -> Result<CoverImage, NormalizeCoverError> {
    if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
        return Err(NormalizeCoverError::Invalid);
    }
    let permit = DECODERS
        .try_acquire()
        .map_err(|_| NormalizeCoverError::Busy)?;
    tokio::task::spawn_blocking(move || {
        // Tie capacity to the blocking work even if the caller drops the request.
        let _permit = permit;
        decode_cover(&bytes).ok_or(NormalizeCoverError::Invalid)
    })
    .await
    .map_err(|_| NormalizeCoverError::Busy)?
}

fn decode_cover(bytes: &[u8]) -> Option<CoverImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
        || decoder.total_bytes() > MAX_DECODED_BYTES
    {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);

    // Crop before resizing to avoid scaling a panoramic source into a giant
    // intermediate bitmap. Orientations and all source metadata stay private.
    let (width, height) = (image.width(), image.height());
    let crop_width = width.min((height * COVER_WIDTH / COVER_HEIGHT).max(1));
    let crop_height = height.min((width * COVER_HEIGHT / COVER_WIDTH).max(1));
    let cropped = image.crop_imm(
        (width - crop_width) / 2,
        (height - crop_height) / 2,
        crop_width,
        crop_height,
    );
    drop(image);
    let rgb = cropped
        .resize_exact(
            COVER_WIDTH,
            COVER_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .into_rgb8();
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, 85)
        .encode_image(&rgb)
        .ok()?;
    Some(CoverImage(output))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

    use super::*;

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 60, Rgb([213, 101, 37])));
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, format).unwrap();
        output.into_inner()
    }

    #[test]
    fn supported_images_are_cropped_to_a_normalized_raster() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let cover = decode_cover(&encoded(format)).unwrap();
            let bytes = cover.into_bytes();
            assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
            let decoded = image::load_from_memory(&bytes).unwrap().into_rgb8();
            assert_eq!(decoded.dimensions(), (COVER_WIDTH, COVER_HEIGHT));
            let rgb = decoded[(COVER_WIDTH / 2, COVER_HEIGHT / 2)];
            assert!(rgb[0] > rgb[1] && rgb[1] > rgb[2]);
        }
    }

    #[tokio::test]
    async fn invalid_and_oversized_inputs_are_rejected() {
        for input in [
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
            encoded(ImageFormat::Png)[..24].to_vec(),
        ] {
            assert!(decode_cover(&input).is_none());
        }
        assert_eq!(
            normalize_cover_image(vec![0; MAX_COVER_BYTES + 1])
                .await
                .unwrap_err(),
            NormalizeCoverError::Invalid
        );
    }

    #[test]
    fn decoded_dimensions_are_bounded_before_resampling() {
        let image = DynamicImage::ImageLuma8(image::GrayImage::new(8193, 1));
        let mut input = Cursor::new(Vec::new());
        image.write_to(&mut input, ImageFormat::Png).unwrap();
        assert!(decode_cover(&input.into_inner()).is_none());
    }
}
