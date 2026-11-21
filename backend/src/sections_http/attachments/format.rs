use std::io::Cursor;

use image::{ImageDecoder, ImageFormat, ImageReader, Limits};

const MAX_IMAGE_SIDE: u32 = 8192;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_IMAGE_MEMORY: u64 = MAX_IMAGE_PIXELS * 8;
const MAX_FILENAME_BYTES: usize = 255;

#[derive(Clone, Copy, Debug)]
pub(super) enum FileFormat {
    Pdf,
    Text,
    Office {
        document: &'static [u8],
        media_type: &'static str,
    },
    Image {
        format: ImageFormat,
        media_type: &'static str,
    },
}

impl FileFormat {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(
            match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
                "pdf" => Self::Pdf,
                "txt" | "md" | "markdown" => Self::Text,
                "docx" => Self::Office {
                    document: b"word/document.xml",
                    media_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                },
                "xlsx" => Self::Office {
                    document: b"xl/workbook.xml",
                    media_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                },
                "pptx" => Self::Office {
                    document: b"ppt/presentation.xml",
                    media_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                },
                "png" => Self::Image {
                    format: ImageFormat::Png,
                    media_type: "image/png",
                },
                "jpg" | "jpeg" => Self::Image {
                    format: ImageFormat::Jpeg,
                    media_type: "image/jpeg",
                },
                "webp" => Self::Image {
                    format: ImageFormat::WebP,
                    media_type: "image/webp",
                },
                _ => return None,
            },
        )
    }

    pub(super) fn accepts_content_type(self, value: &str) -> bool {
        let value = value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        value == "application/octet-stream"
            || match self {
                Self::Pdf => value == "application/pdf",
                Self::Text => matches!(
                    value.as_str(),
                    "text/plain" | "text/markdown" | "text/x-markdown"
                ),
                Self::Office { media_type, .. } => {
                    value == media_type || value == "application/zip"
                }
                Self::Image { media_type, .. } => value == media_type,
            }
    }

    // Return an inline type only for a decoded, bounded raster. Other formats
    // are always sent as a download, and their original bytes are preserved.
    pub(super) fn verify(self, bytes: &[u8]) -> Option<Option<&'static str>> {
        match self {
            Self::Pdf => valid_pdf(bytes).then_some(None),
            Self::Text => is_text(bytes).then_some(None),
            Self::Office { document, .. } => office_package(bytes, document).then_some(None),
            Self::Image { format, media_type } => raster(bytes, format).then_some(Some(media_type)),
        }
    }
}

pub(super) fn decode_name(encoded: &str) -> Option<String> {
    if encoded.len() > 3 * MAX_FILENAME_BYTES {
        return None;
    }
    let mut output = Vec::with_capacity(encoded.len());
    let mut chars = encoded.bytes();
    while let Some(ch) = chars.next() {
        if ch == b'%' {
            let high = char::from(chars.next()?).to_digit(16)?;
            let low = char::from(chars.next()?).to_digit(16)?;
            output.push((high * 16 + low) as u8);
        } else {
            output.push(ch);
        }
    }
    let name = String::from_utf8(output).ok()?;
    (!name.trim().is_empty()
        && name.len() <= MAX_FILENAME_BYTES
        && !matches!(name.trim(), "." | "..")
        && !name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\')))
    .then_some(name)
}

fn is_text(bytes: &[u8]) -> bool {
    if let Some(encoded) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return utf16_text(encoded, u16::from_le_bytes);
    }
    if let Some(encoded) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return utf16_text(encoded, u16::from_be_bytes);
    }
    // This also allows common legacy Cyrillic encodings. The bytes are an
    // opaque download and never parsed as markup.
    !bytes
        .iter()
        .any(|byte| matches!(byte, 0..=8 | 11 | 12 | 14..=31))
}

fn utf16_text(bytes: &[u8], code_unit: fn([u8; 2]) -> u16) -> bool {
    let (units, remainder) = bytes.as_chunks::<2>();
    remainder.is_empty()
        && char::decode_utf16(units.iter().copied().map(code_unit)).all(|char| {
            char.is_ok_and(|char| matches!(char, '\t' | '\n' | '\r') || !char.is_control())
        })
}

fn valid_pdf(bytes: &[u8]) -> bool {
    if !matches!(
        bytes.get(..9),
        Some(
            [
                b'%',
                b'P',
                b'D',
                b'F',
                b'-',
                b'1',
                b'.',
                b'0'..=b'7',
                b'\r' | b'\n'
            ] | [
                b'%',
                b'P',
                b'D',
                b'F',
                b'-',
                b'2',
                b'.',
                b'0',
                b'\r' | b'\n'
            ]
        )
    ) {
        return false;
    }
    let tail = &bytes[bytes.len().saturating_sub(1024)..];
    let trimmed = tail.trim_ascii_end();
    trimmed
        .strip_suffix(b"%%EOF")
        .and_then(|before| before.last())
        .is_some_and(|before| matches!(before, b'\r' | b'\n'))
}

fn raster(bytes: &[u8], expected: ImageFormat) -> bool {
    let Ok(mut reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() else {
        return false;
    };
    if reader.format() != Some(expected) {
        return false;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_MEMORY);
    reader.limits(limits);
    let Ok(decoder) = reader.into_decoder() else {
        return false;
    };
    let (width, height) = decoder.dimensions();
    width > 0
        && height > 0
        && u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS
        && decoder.total_bytes() <= MAX_IMAGE_MEMORY
        && image::DynamicImage::from_decoder(decoder).is_ok()
}

fn zip_word(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize)
}

fn zip_dword(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as usize)
}

fn office_package(bytes: &[u8], document: &[u8]) -> bool {
    if !bytes.starts_with(b"PK\x03\x04") {
        return false;
    }
    office_entries(bytes, document).unwrap_or(false)
}

fn office_entries(bytes: &[u8], document: &[u8]) -> Option<bool> {
    // Inspect the directory (no extraction or XML parsing), so compressed
    // archives have a fixed cost relative to the received bytes.
    let tail = bytes.len().saturating_sub(u16::MAX as usize + 22);
    let end = bytes[tail..]
        .windows(4)
        .enumerate()
        .rev()
        .find_map(|(offset, tag)| {
            let end = tail + offset;
            (tag == b"PK\x05\x06" && end + 22 + zip_word(bytes, end + 20)? == bytes.len())
                .then_some(end)
        })?;
    let record = &bytes[end..end + 22];
    let entries = zip_word(record, 10)?;
    if zip_word(record, 4)? != 0 || zip_word(record, 6)? != 0 || zip_word(record, 8)? != entries {
        return None;
    }
    let start = zip_dword(record, 16)?;
    if start.checked_add(zip_dword(record, 12)?)? != end {
        return None;
    }
    let mut directory = bytes.get(start..end)?;
    let mut has_manifest = false;
    let mut has_document = false;
    for _ in 0..entries {
        let header = directory.get(..46)?;
        if !header.starts_with(b"PK\x01\x02") {
            return None;
        }
        let length = zip_word(header, 28)?;
        let name = directory.get(46..46 + length)?;
        let manifest = name == b"[Content_Types].xml";
        let expected_document = name == document;
        if manifest || expected_document {
            if manifest && has_manifest || expected_document && has_document {
                return None;
            }
            bounded_local_entry(&bytes[..start], header, name)?;
            has_manifest |= manifest;
            has_document |= expected_document;
        }
        directory = directory.get(46 + length + zip_word(header, 30)? + zip_word(header, 32)?..)?;
    }
    Some(directory.is_empty() && has_manifest && has_document)
}

fn bounded_local_entry(area: &[u8], central: &[u8], name: &[u8]) -> Option<()> {
    let local = area.get(zip_dword(central, 42)?..)?;
    if !local.starts_with(b"PK\x03\x04") {
        return None;
    }
    let flags = zip_word(central, 8)?;
    let packed = zip_dword(central, 20)?;
    let unpacked = zip_dword(central, 24)?;
    if flags & 1 != 0
        || packed == 0
        || unpacked == 0
        || zip_word(local, 6)? != flags
        || zip_word(local, 8)? != zip_word(central, 10)?
        || zip_word(local, 26)? != name.len()
        || local.get(30..30 + name.len())? != name
    {
        return None;
    }
    // With a data descriptor, the local header may omit the sizes.
    if flags & 8 == 0 && (zip_dword(local, 18)? != packed || zip_dword(local, 22)? != unpacked) {
        return None;
    }
    let payload = 30 + name.len() + zip_word(local, 28)?;
    local.get(payload..payload.checked_add(packed)?)?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_round_trip_without_header_injection() {
        assert_eq!(decode_name("%D0%90%20%2B.txt").as_deref(), Some("А +.txt"));
        for name in [
            "..",
            "a%2Fb.txt",
            "a%5Cb.txt",
            "%",
            "%GG.txt",
            "%FF.txt",
            "x%0d%0ay.txt",
        ] {
            assert_eq!(decode_name(name), None, "{name}");
        }
        assert_eq!(
            crate::http::content_disposition("А +.txt", false),
            "attachment; filename=\"attachment\"; filename*=UTF-8''%D0%90%20%2B.txt"
        );
    }

    #[test]
    fn raster_validation_requires_the_declared_format_and_complete_bounded_image() {
        let mut bytes = Cursor::new(Vec::new());
        let bitmap = image::DynamicImage::ImageRgb8(image::RgbImage::new(2, 3));
        bitmap.write_to(&mut bytes, ImageFormat::Png).unwrap();
        let bytes = bytes.into_inner();
        assert_eq!(
            FileFormat::from_name("Фото.PNG").unwrap().verify(&bytes),
            Some(Some("image/png"))
        );
        assert_eq!(
            FileFormat::from_name("Фото.jpg").unwrap().verify(&bytes),
            None
        );
        assert_eq!(
            FileFormat::from_name("Фото.png")
                .unwrap()
                .verify(&bytes[..24]),
            None
        );

        let mut oversized = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::new(MAX_IMAGE_SIDE + 1, 1))
            .write_to(&mut oversized, ImageFormat::Png)
            .unwrap();
        assert!(!raster(oversized.get_ref(), ImageFormat::Png));
    }

    #[test]
    fn accepts_markdown_extension_and_requires_pdf_structure() {
        assert_eq!(
            FileFormat::from_name("пример.Markdown")
                .unwrap()
                .verify("# Заголовок".as_bytes()),
            Some(None)
        );
        for pdf in [
            b"%PDF-1.4\n1 0 obj\nendobj\n%%EOF\n".as_slice(),
            b"%PDF-2.0\r\n%%EOF\r\n ",
        ] {
            assert_eq!(FileFormat::Pdf.verify(pdf), Some(None));
        }
        for malformed in [
            b"%PDF-".as_slice(),
            b"%PDF-9.0\n%%EOF",
            b"%PDF-1.7\ntruncated",
            b"%PDF-1.7\nnot-a-line%%EOF",
        ] {
            assert_eq!(FileFormat::Pdf.verify(malformed), None);
        }
    }

    fn package(names: &[&[u8]]) -> Vec<u8> {
        let mut offsets = Vec::new();
        let mut bytes = Vec::new();
        for name in names {
            offsets.push(bytes.len() as u32);
            let mut local = [0; 30];
            local[..4].copy_from_slice(b"PK\x03\x04");
            local[18..22].copy_from_slice(&6u32.to_le_bytes());
            local[22..26].copy_from_slice(&6u32.to_le_bytes());
            local[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend(local);
            bytes.extend(*name);
            bytes.extend(b"<xml/>");
        }
        let start = bytes.len();
        for (name, offset) in names.iter().zip(offsets) {
            let mut header = [0; 46];
            header[..4].copy_from_slice(b"PK\x01\x02");
            header[20..24].copy_from_slice(&6u32.to_le_bytes());
            header[24..28].copy_from_slice(&6u32.to_le_bytes());
            header[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
            header[42..46].copy_from_slice(&offset.to_le_bytes());
            bytes.extend(header);
            bytes.extend(*name);
        }
        let len = bytes.len() - start;
        let mut end = [0; 22];
        end[..4].copy_from_slice(b"PK\x05\x06");
        end[8..10].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[10..12].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[12..16].copy_from_slice(&(len as u32).to_le_bytes());
        end[16..20].copy_from_slice(&(start as u32).to_le_bytes());
        bytes.extend(end);
        bytes
    }

    #[test]
    fn office_download_matches_package_directory() {
        let docx = FileFormat::from_name("Notes.docx").unwrap();
        assert_eq!(
            docx.verify(&package(&[b"[Content_Types].xml", b"word/document.xml"])),
            Some(None)
        );
        assert_eq!(
            docx.verify(&package(&[b"[Content_Types].xml", b"xl/workbook.xml"])),
            None
        );
        assert_eq!(
            docx.verify(b"PK\x03\x04 [Content_Types].xml word/document.xml"),
            None
        );
        let mut mismatch = package(&[b"[Content_Types].xml", b"word/document.xml"]);
        mismatch[30] = b'?';
        assert_eq!(docx.verify(&mismatch), None);
        let mut out_of_bounds = package(&[b"[Content_Types].xml", b"word/document.xml"]);
        let eocd = out_of_bounds.len() - 22;
        let central = zip_dword(&out_of_bounds[eocd..], 16).unwrap();
        out_of_bounds[central + 42..central + 46].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(docx.verify(&out_of_bounds), None);
    }
}
