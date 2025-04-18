//! Archive extracters

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use crate::error::{Error, ErrorKind, Result, WithDesc};

#[cfg(feature = "zip")]
pub mod zip;

#[cfg(feature = "tar")]
pub mod tar;

fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).with_desc("Failed to create directory")?;
    }
    Ok(())
}

/// A trait for archive formats that can be extracted.
///
/// Implementers of this trait can extract files from an archive to the filesystem,
/// using a mapper function to determine whether and where to extract each file.
pub trait Archive {
    /// Extracts the archive contents.
    ///
    /// # Parameters
    ///
    /// * `mapper` - A function that takes a path from the archive and returns either a destination
    ///   path to extract the file to, or `None` to skip extracting this file.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the extraction.
    fn extract(self, mapper: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()>;
}

/// A archive file on disk
#[derive(Debug, Clone, Copy)]
pub struct ArchiveFile<'a>(&'a Path);

impl<'a> ArchiveFile<'a> {
    /// Creates a new `ArchiveFile` instance.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to the archive file.
    ///
    /// # Returns
    ///
    /// A new `ArchiveFile` instance.
    pub fn new(path: &'a Path) -> Self {
        Self(path)
    }
}

impl Archive for ArchiveFile<'_> {
    /// Converts the `ArchiveFile` into an `Archive` instance.
    ///
    /// # Returns
    ///
    /// An `Archive` instance.
    fn extract(self, mapper: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()> {
        let file = std::fs::File::open(self.0)?;
        let ext = get_extension(self.0)
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| {
                Error::new(ErrorKind::Extract)
                    .with_desc(format!("Unknown archive format {}", self.0.display()))
            })?;

        // Determine the archive format based on the file extension
        match ext {
            #[cfg(feature = "zip")]
            "zip" => ::zip::ZipArchive::new(file)?.extract(mapper),
            #[cfg(feature = "tar")]
            "tar" => ::tar::Archive::new(file).extract(mapper),
            #[cfg(all(feature = "tar", feature = "deflate"))]
            "tgz" | "tar.gz" => tar::gz::Archive::new(file).extract(mapper),
            _ => Err(Error::new(ErrorKind::Extract)
                .with_desc(format!("Unsupported archive format {}", self.0.display()))),
        }
    }
}

/// Get full extension from a path
fn get_extension(path: &Path) -> Option<&OsStr> {
    let file = path.file_name()?;

    let slice = file.as_encoded_bytes();
    if slice == b".." {
        return None;
    }

    let i = slice[1..].iter().position(|b| *b == b'.')?;
    let extension_bytes = &slice[i + 2..];
    // Safety: The bytes are valid UTF-8 because they were obtained from an OsStr.
    unsafe { Some(OsStr::from_encoded_bytes_unchecked(extension_bytes)) }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn test_get_extension() {
        let path = Path::new("foo/bar.zip");
        assert_eq!(get_extension(path), Some(OsStr::new("zip")));

        let path = Path::new("bar.zip");
        assert_eq!(get_extension(path), Some(OsStr::new("zip")));

        let path = Path::new("bar.tar.gz");
        assert_eq!(get_extension(path), Some(OsStr::new("tar.gz")));

        let path = Path::new("bar");
        assert_eq!(get_extension(path), None);

        let path = Path::new(".");
        assert_eq!(get_extension(path), None);

        let path = Path::new("..");
        assert_eq!(get_extension(path), None);
    }
}
