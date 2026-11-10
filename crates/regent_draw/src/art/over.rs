//! The override folder: PNGs a user drops in to replace interface
//! pictures, named as `regent_cli ui-dump` writes them
//! (`regent_core::view::picture_file`). Nothing is shipped there; openregent
//! names the folder ([`DIR`] under its data directory).
//!
//! ```text
//!  <dir>/<archive>/<IMAG>_<set>_s<slot>_f<frame>.png
//!  <dir>/portraits/<archive>/…                     the same (a dump's folder)
//!  <dir>/tiles/<archive>_<n>.png
//! ```
//!
//! Scanned at start and when the settings editor closes: names, sizes and
//! times only; a file is read when its picture is first drawn. A file of
//! any size is its picture's pixels at its own resolution.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy_egui::egui::ColorImage;
use regent_core::data::fnv;
use regent_core::view::picture_file::{EXTENSION, Key};

/// The folder's name under the program's data directory.
pub const DIR: &str = "art";

/// A file as found: where, how long, when last written. A change in any
/// is a new picture.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct File {
    pub path: PathBuf,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

/// The overrides found by one scan, by the picture each names.
#[derive(Default)]
pub struct Overrides {
    files: BTreeMap<Key, File>,
    /// Files that would not read: logged once, then left out.
    bad: HashSet<Key>,
}

impl Overrides {
    /// The files in `dir` and its folders naming a picture; a PNG naming
    /// none is logged. No folder: none.
    #[must_use]
    pub fn scan(dir: &Path) -> Overrides {
        let mut files = BTreeMap::new();
        let mut folders = vec![dir.to_path_buf()];

        while let Some(folder) = folders.pop() {
            let entries = std::fs::read_dir(&folder).into_iter().flatten().flatten();

            for entry in entries {
                let path = entry.path();
                let meta = entry.metadata().ok();
                let name = relative(dir, &path);

                if path.is_dir() {
                    folders.push(path);
                } else if let (Some(meta), Some(key)) =
                    (&meta, name.as_deref().and_then(Key::parse))
                {
                    let file = File {
                        path,
                        len: meta.len(),
                        modified: meta.modified().ok(),
                    };

                    files.insert(key, file);
                } else if path.extension().is_some_and(|e| e == EXTENSION) {
                    tracing::warn!("art: {}: names no picture, left out", path.display());
                }
            }
        }

        Overrides {
            files,
            bad: HashSet::new(),
        }
    }

    /// How many files name a picture.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Whether a file names one of `keys` and has not failed to read.
    #[must_use]
    pub fn holds(&self, keys: &[Key]) -> bool {
        keys.iter()
            .any(|k| self.files.contains_key(k) && !self.bad.contains(k))
    }

    /// Every file's path, length and time hashed: what the art held was
    /// made under. No files: 0.
    #[must_use]
    pub fn identity(&self) -> u64 {
        if self.files.is_empty() {
            0
        } else {
            let bytes = self.files.values().flat_map(|f| {
                let when = f
                    .modified
                    .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());

                f.path
                    .to_string_lossy()
                    .bytes()
                    .chain(f.len.to_le_bytes())
                    .chain(when.to_le_bytes())
                    .collect::<Vec<u8>>()
            });

            fnv::fold(fnv::START, bytes)
        }
    }

    /// The pixels of the first of `keys` with a file that reads; a file
    /// that does not is logged the first time.
    pub fn read(&mut self, keys: &[Key]) -> Option<ColorImage> {
        keys.iter().find_map(|key| {
            let file = self.files.get(key).filter(|_| !self.bad.contains(key))?;

            match load(&file.path) {
                Ok(image) => Some(image),
                Err(e) => {
                    tracing::warn!("art: {e}, left out");
                    self.bad.insert(key.clone());
                    None
                }
            }
        })
    }
}

/// `path` under `dir`, `/` between folders.
fn relative(dir: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path
        .strip_prefix(dir)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect();

    parts.map(|p| p.join("/"))
}

/// A PNG's pixels, any colour type, 8 or 16 bits.
fn load(path: &Path) -> Result<ColorImage, String> {
    let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let bytes = std::fs::read(path).map_err(|e| shown(&e))?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));

    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().map_err(|e| shown(&e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| shown(&"too large"))?;
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer).map_err(|e| shown(&e))?;
    let pixels = &buffer[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], u8::MAX])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, u8::MAX]).collect(),
        png::ColorType::Indexed => return Err(shown(&"indexed colour left unexpanded")),
    };
    let size = [info.width, info.height].map(|n| usize::try_from(n).unwrap_or_default());

    if size.contains(&0) || rgba.len() != size[0] * size[1] * 4 {
        Err(shown(&"no pixels"))
    } else {
        Ok(ColorImage::from_rgba_unmultiplied(size, &rgba))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A scratch folder beside the test binary, emptied.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::current_exe()
            .expect("the test binary")
            .with_file_name(name);

        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// `image` written as a PNG at `path`.
    pub(crate) fn write(path: &Path, image: &ColorImage) {
        let rgba: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(bevy_egui::egui::Color32::to_srgba_unmultiplied)
            .collect();
        let [w, h] = image.size.map(|n| u32::try_from(n).expect("small"));
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, w, h);

        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&rgba))
            .expect("encodes");
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("a folder");
        std::fs::write(path, bytes).expect("written");
    }

    /// A scan finds the files naming pictures, by their keys, and leaves
    /// out the rest; no folder is no overrides.
    #[test]
    fn a_scan_keys_files_by_the_picture_they_name() {
        let dir = scratch("art-over-scan");
        let image = ColorImage::filled([2, 2], bevy_egui::egui::Color32::RED);

        write(&dir.join("interfacedata/AP29_8003_s0_f0.png"), &image);
        write(&dir.join("tiles/interfacedata_7.png"), &image);
        write(&dir.join("interfacedata/notes.png"), &image);
        std::fs::write(dir.join("README.md"), "how to").expect("written");

        let found = Overrides::scan(&dir);

        assert_eq!(found.len(), 2);
        assert!(found.files.contains_key(&Key::tile("interfacedata.cam", 7)));
        assert_ne!(found.identity(), 0);
        assert!(Overrides::scan(&dir.join("absent")).is_empty());
        assert_eq!(Overrides::scan(&dir.join("absent")).identity(), 0);
    }
}
