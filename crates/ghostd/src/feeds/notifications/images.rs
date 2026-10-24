//! `image-data` hints to PNG files the shell can load. The hint is untrusted: sizes and strides are
//! checked before any pixel is read.

use std::fs::{self, DirBuilder};
use std::io::BufWriter;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use thiserror::Error;
use tracing::warn;
use zbus::Variant;

const BITS_PER_SAMPLE: i32 = 8;
const RGB_CHANNELS: i32 = 3;
const RGBA_CHANNELS: i32 = 4;
/// Calls 6: a 4 096 × 4 096 image is already 64 MiB of RGBA, which no toast needs.
const MAX_PIXELS: i64 = 4_096 * 4_096;
const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const EXTENSION: &str = "png";
const PARTIAL_EXTENSION: &str = "png.part";

/// The `(iiibiiay)` struct of the Desktop Notifications spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub channels: i32,
    pub data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("{bits_per_sample} bits per sample and {channels} channels are not 8-bit RGB or RGBA")]
    Format { bits_per_sample: i32, channels: i32 },
    #[error("{width} × {height} is empty or past the {MAX_PIXELS} pixel limit")]
    Size { width: i32, height: i32 },
    #[error("rowstride {rowstride} and {len} bytes cannot hold {height} rows of {row_bytes} bytes")]
    Rows {
        rowstride: i32,
        len: usize,
        height: i32,
        row_bytes: usize,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Encode(#[from] png::EncodingError),
}

impl ImageData {
    /// `None` when `value` is not the `(iiibiiay)` struct.
    pub fn from_variant(value: &Variant<'_>) -> Option<Self> {
        let Variant::Structure(structure) = value else {
            return None;
        };
        let [
            width,
            height,
            rowstride,
            has_alpha,
            bits_per_sample,
            channels,
            data,
        ] = structure.fields()
        else {
            return None;
        };
        Some(Self {
            width: width.downcast_ref().ok()?,
            height: height.downcast_ref().ok()?,
            rowstride: rowstride.downcast_ref().ok()?,
            has_alpha: has_alpha.downcast_ref().ok()?,
            bits_per_sample: bits_per_sample.downcast_ref().ok()?,
            channels: channels.downcast_ref().ok()?,
            data: Vec::<u8>::try_from(data.try_clone().ok()?).ok()?,
        })
    }

    /// The pixels with the row padding removed.
    fn packed(&self) -> Result<(png::ColorType, Vec<u8>), ImageError> {
        let color = match (self.bits_per_sample, self.channels) {
            (BITS_PER_SAMPLE, RGB_CHANNELS) => png::ColorType::Rgb,
            (BITS_PER_SAMPLE, RGBA_CHANNELS) => png::ColorType::Rgba,
            (bits_per_sample, channels) => {
                return Err(ImageError::Format {
                    bits_per_sample,
                    channels,
                });
            }
        };
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        if width <= 0 || height <= 0 || width * height > MAX_PIXELS {
            return Err(ImageError::Size {
                width: self.width,
                height: self.height,
            });
        }
        // Both fit: the pixel limit bounds them and `channels` is 3 or 4.
        let row_bytes = usize::try_from(width * i64::from(self.channels)).unwrap_or(usize::MAX);
        let rows = usize::try_from(height).unwrap_or(usize::MAX);
        let stride = usize::try_from(self.rowstride).unwrap_or(0);
        // The last row may stop at its pixels, without the padding.
        let needed = stride.saturating_mul(rows - 1).saturating_add(row_bytes);
        if stride < row_bytes || self.data.len() < needed {
            return Err(ImageError::Rows {
                rowstride: self.rowstride,
                len: self.data.len(),
                height: self.height,
                row_bytes,
            });
        }
        let mut packed = Vec::with_capacity(row_bytes * rows);
        for row in 0..rows {
            let start = row * stride;
            packed.extend_from_slice(&self.data[start..start + row_bytes]);
        }
        Ok((color, packed))
    }
}

/// Writes `image` to `<dir>/<id>.png`, mode 0600, creating `dir` with mode 0700 when missing.
pub fn write_png(dir: &Path, id: u32, image: &ImageData) -> Result<PathBuf, ImageError> {
    let (color, pixels) = image.packed()?;
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(dir)?;
    let path = dir.join(format!("{id}.{EXTENSION}"));
    // Renamed into place so the shell never loads half a file.
    let partial = dir.join(format!("{id}.{PARTIAL_EXTENSION}"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FILE_MODE)
        .open(&partial)?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        image.width.unsigned_abs(),
        image.height.unsigned_abs(),
    );
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    fs::rename(&partial, &path)?;
    Ok(path)
}

/// Removes the PNGs of every id not in `keep`.
pub fn prune(dir: &Path, keep: &[u32]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(&format!(".{EXTENSION}")))
            .and_then(|stem| stem.parse::<u32>().ok());
        if id.is_some_and(|id| !keep.contains(&id))
            && let Err(error) = fs::remove_file(&path)
        {
            warn!(%error, path = %path.display(), "cannot remove a stale notification image");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    const SIDE: i32 = 2;
    const PIXEL: [u8; 4] = [10, 20, 30, 255];

    fn rgba(rowstride: i32, data: Vec<u8>) -> ImageData {
        ImageData {
            width: SIDE,
            height: SIDE,
            rowstride,
            has_alpha: true,
            bits_per_sample: BITS_PER_SAMPLE,
            channels: RGBA_CHANNELS,
            data,
        }
    }

    fn decode(path: &Path) -> (png::OutputInfo, Vec<u8>) {
        let mut reader = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        (info, pixels)
    }

    #[test]
    fn rgba_rows_become_a_png() {
        let dir = tempfile::tempdir().unwrap();
        // A 4-byte pad after each 8-byte row, which the PNG must not carry.
        let mut data = Vec::new();
        for _ in 0..SIDE {
            data.extend(PIXEL.repeat(SIDE as usize));
            data.extend([0xEE; 4]);
        }
        let path = write_png(&dir.path().join("notify"), 7, &rgba(12, data)).unwrap();
        assert_eq!(path, dir.path().join("notify/7.png"));
        let (info, pixels) = decode(&path);
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&pixels[..16], PIXEL.repeat(4).as_slice());
        let mode =
            std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&path).unwrap().permissions());
        assert_eq!(mode & 0o777, FILE_MODE);
    }

    #[test]
    fn bad_image_rows_are_logged_not_written() {
        let dir = tempfile::tempdir().unwrap();
        // 7 < 2 pixels × 4 channels.
        let result = write_png(dir.path(), 1, &rgba(7, vec![0; 64]));
        assert!(matches!(result, Err(ImageError::Rows { .. })));
        let short = write_png(dir.path(), 2, &rgba(8, vec![0; 8]));
        assert!(matches!(short, Err(ImageError::Rows { .. })));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn oversized_image_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let huge = ImageData {
            width: 20_000,
            height: 20_000,
            rowstride: 80_000,
            ..rgba(0, Vec::new())
        };
        assert!(matches!(
            write_png(dir.path(), 1, &huge),
            Err(ImageError::Size { .. })
        ));
        let sixteen_bit = ImageData {
            bits_per_sample: 16,
            ..rgba(8, vec![0; 16])
        };
        assert!(matches!(
            write_png(dir.path(), 1, &sixteen_bit),
            Err(ImageError::Format { .. })
        ));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn prune_removes_files_not_in_keep() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["1.png", "2.png", "3.png", "notes.txt"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        prune(dir.path(), &[2]);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["2.png", "notes.txt"]);
    }
}
