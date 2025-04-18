use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use crate::{
    error::{Error, ErrorKind, Result, WithDesc},
    extract::{Archive, ensure_dir_exists},
};

impl<R: Read + Seek> Archive for ::zip::ZipArchive<R> {
    fn extract(mut self, mut mapper: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()> {
        for i in 0..self.len() {
            let mut file = self
                .by_index(i)
                .with_desc("Failed to get file from zip archive")?;

            let src_path = file.enclosed_name().ok_or_else(|| {
                Error::new(ErrorKind::Extract).with_desc("Bad file path in zip archive")
            })?;
            let dst = match mapper(&src_path) {
                Some(path) => path,
                None => continue,
            };
            let dst = dst.as_path();

            if file.is_dir() {
                continue;
            }

            if let Some(dir) = dst.parent() {
                ensure_dir_exists(dir)?;
            }

            // Resolve symlinks
            #[cfg(unix)]
            {
                use std::os::unix::{ffi::OsStringExt, fs::symlink};

                const S_IFLNK: u32 = 0o120000;

                if let Some(mode) = file.unix_mode() {
                    if mode & S_IFLNK == S_IFLNK {
                        let mut contents = Vec::new();
                        file.read_to_end(&mut contents)?;
                        let link_target = std::ffi::OsString::from_vec(contents);
                        if dst.exists() {
                            std::fs::remove_file(dst)?;
                        }
                        symlink(link_target, dst).then_with_desc(|| {
                            format!("Failed to extract file: {}", dst.display())
                        })?;
                        continue;
                    }
                }
            }

            let mut outfile = File::create(dst)
                .then_with_desc(|| format!("Failed to create file: {}", dst.display()))?;
            std::io::copy(&mut file, &mut outfile)
                .then_with_desc(|| format!("Failed to extract file: {}", dst.display()))?;

            #[cfg(unix)]
            {
                use std::{
                    fs::{Permissions, set_permissions},
                    os::unix::fs::PermissionsExt,
                };

                if let Some(mode) = file.unix_mode() {
                    set_permissions(dst, Permissions::from_mode(mode)).then_with_desc(|| {
                        format!("Failed to set permissions: {}", dst.display())
                    })?;
                }
            }
        }

        Ok(())
    }
}

impl From<::zip::result::ZipError> for Error {
    fn from(err: ::zip::result::ZipError) -> Self {
        Error::new(ErrorKind::Extract).with_source(err)
    }
}
