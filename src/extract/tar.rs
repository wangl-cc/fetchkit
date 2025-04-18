use std::{
    io::Read,
    path::{Path, PathBuf},
};

use super::{Archive, ensure_dir_exists};
use crate::error::{Result, WithDesc};

impl<R: Read> Archive for ::tar::Archive<R> {
    fn extract(mut self, mut mapper: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()> {
        for entry in self
            .entries()
            .with_desc("Failed to read file entry in archive")?
        {
            let mut entry = entry.with_desc("Invalid file entry in archive")?;
            let entry_path = entry.path().with_desc("Invalid file path in archive")?;
            let dst = match mapper(entry_path.as_ref()) {
                Some(path) => path,
                None => continue,
            };

            if let Some(parent) = dst.parent() {
                ensure_dir_exists(parent)?;
            }

            entry.unpack(&dst)?;
        }

        Ok(())
    }
}

#[cfg(feature = "deflate")]
pub mod gz {
    use super::{Archive as ArchiveTrait, *};

    pub struct Archive<R> {
        archive: flate2::read::GzDecoder<R>,
    }

    impl<R: Read> Archive<R> {
        pub fn new(reader: R) -> Self {
            Self {
                archive: flate2::read::GzDecoder::new(reader),
            }
        }
    }

    impl<R: Read> ArchiveTrait for Archive<R> {
        fn extract(self, mapper: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()> {
            ::tar::Archive::new(self.archive).extract(mapper)
        }
    }
}
