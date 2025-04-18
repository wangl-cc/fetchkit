use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use fetchkit::{
    error::{ErrorKind, Result},
    extract::{Archive, ArchiveFile},
};
use tempfile::TempDir;

// Helper function to create a directory structure with files for testing
fn create_test_files(dir: &Path) -> Result<()> {
    // Create a simple directory structure
    let subdir_path = dir.join("subdir");
    fs::create_dir_all(&subdir_path)?;

    // Create a file in the root directory
    let file1_path = dir.join("file1.txt");
    let mut file1 = File::create(&file1_path)?;
    file1.write_all(b"This is file 1")?;

    // Create a file in the subdirectory
    let file2_path = subdir_path.join("file2.txt");
    let mut file2 = File::create(&file2_path)?;
    file2.write_all(b"This is file 2")?;

    // Create a symbolic link to file1.txt on unix
    #[cfg(unix)]
    {
        let link_path = dir.join("file1_link.txt");
        std::os::unix::fs::symlink("file1.txt", link_path)?;
    }

    verify_identity_extraction(dir)?;

    Ok(())
}

// Helper function to create the identity mapper
fn identity_mapper(output_dir: &Path) -> impl FnMut(&Path) -> Option<PathBuf> {
    move |path: &Path| Some(output_dir.join(path))
}

// Helper function to verify extracted files with identity mapper
#[track_caller]
fn verify_identity_extraction(extract_dir: &Path) -> Result<()> {
    let file1_path = extract_dir.join("file1.txt");
    let file2_path = extract_dir.join("subdir").join("file2.txt");
    let file1_link_path = extract_dir.join("file1_link.txt");

    // Check that files exist
    assert!(
        file1_path.is_file(),
        "file1.txt should be a file ({:?})",
        file1_path.metadata()
    );
    assert!(
        file2_path.is_file(),
        "file2.txt should be a file ({:?})",
        file2_path.metadata()
    );
    #[cfg(unix)]
    assert!(
        file1_link_path.is_symlink(),
        "file1_link.txt should be a symlink {:?}",
        file1_link_path.metadata()
    );

    // Check file contents
    let mut content = String::new();
    File::open(file1_path)?.read_to_string(&mut content)?;
    assert_eq!(content, "This is file 1", "file1.txt content mismatch");

    content.clear();
    File::open(file2_path)?.read_to_string(&mut content)?;
    assert_eq!(content, "This is file 2", "file2.txt content mismatch");

    #[cfg(unix)]
    {
        content.clear();
        File::open(file1_link_path)
            .unwrap()
            .read_to_string(&mut content)?;
        assert_eq!(content, "This is file 1", "file1_link.txt content mismatch");
    }

    Ok(())
}

// Helper function to create a selective mapper that only extracts certain files
fn selective_mapper(output_dir: &Path) -> impl FnMut(&Path) -> Option<PathBuf> {
    move |path: &Path| {
        if path.file_name().is_some_and(|f| f == "file1.txt") {
            Some(output_dir.join(path))
        } else {
            None
        }
    }
}

// Helper function to verify extracted files with selective mapper
#[track_caller]
fn verify_selective_extraction(extract_dir: &Path) -> Result<()> {
    let file1_path = extract_dir.join("file1.txt");
    let file2_path = extract_dir.join("subdir/file2.txt");

    // Check file1.txt exists
    assert!(file1_path.exists(), "file1.txt should exist");

    // Check file2.txt does NOT exist
    assert!(!file2_path.exists(), "file2.txt should NOT exist");

    // Check file1.txt content
    let mut content = String::new();
    File::open(file1_path)?.read_to_string(&mut content)?;
    assert_eq!(content, "This is file 1", "file1.txt content mismatch");

    Ok(())
}

fn walk_dir(dir: &Path, callback: &mut dyn FnMut(&Path) -> Result<()>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        callback(&path)?;
        if path.is_dir() {
            walk_dir(&path, callback)?;
        }
    }
    Ok(())
}

#[cfg(feature = "zip")]
mod zip_tests {
    use zip::{ZipWriter, write::FileOptions};

    use super::*;

    fn create_zip_archive(source_dir: &Path, archive_path: &Path) -> Result<()> {
        let file = File::create(archive_path)?;
        let mut writer = ZipWriter::new(file);

        let options: FileOptions<()> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        walk_dir(source_dir, &mut |path| {
            let relpath = path.strip_prefix(source_dir).unwrap();
            match path {
                path if path.is_dir() => {
                    writer.add_directory_from_path(relpath, options)?;
                }
                // Handle symlinks
                path if path.is_symlink() => {
                    let target = path.read_link()?;
                    writer.add_symlink_from_path(relpath, target, options)?;
                }
                path if path.is_file() => {
                    writer.start_file_from_path(relpath, options)?;
                    let mut file = File::open(path)?;
                    std::io::copy(&mut file, &mut writer)?;
                }
                _ => {}
            }
            Ok(())
        })?;

        writer.finish()?;
        Ok(())
    }

    #[test]
    fn test_extract_zip_identity_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.zip");
        create_zip_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using identity mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(identity_mapper(extract_dir.path()))?;

        // Verify extracted files
        verify_identity_extraction(extract_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_extract_zip_selective_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.zip");
        create_zip_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using selective mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(selective_mapper(extract_dir.path()))?;

        // Verify selective extraction
        verify_selective_extraction(extract_dir.path())?;

        Ok(())
    }
}

#[cfg(feature = "tar")]
mod tar_tests {
    use super::*;

    fn create_tar_archive(source_dir: &Path, archive_path: &Path) -> Result<()> {
        let file = File::create(archive_path)?;
        let mut builder = ::tar::Builder::new(file);
        builder.follow_symlinks(false);

        // Add directory contents to the archive
        builder.append_dir_all(".", source_dir)?;

        builder.finish()?;
        Ok(())
    }

    #[test]
    fn test_extract_tar_identity_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.tar");
        create_tar_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using identity mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(identity_mapper(extract_dir.path()))?;

        // Verify extracted files
        verify_identity_extraction(extract_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_extract_tar_selective_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.tar");
        create_tar_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using selective mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(selective_mapper(extract_dir.path()))?;

        // Verify selective extraction
        verify_selective_extraction(extract_dir.path())?;

        Ok(())
    }
}

#[cfg(all(feature = "tar", feature = "deflate"))]
mod tar_gz_tests {
    use std::io::BufWriter;

    use super::*;

    fn create_tar_gz_archive(source_dir: &Path, archive_path: &Path) -> Result<()> {
        let file = File::create(archive_path)?;
        let buf_writer = BufWriter::new(file);
        let gz_encoder = flate2::write::GzEncoder::new(buf_writer, flate2::Compression::default());
        let mut builder = ::tar::Builder::new(gz_encoder);
        builder.follow_symlinks(false);

        // Add directory contents to the archive
        builder.append_dir_all(".", source_dir)?;

        builder.finish()?;
        Ok(())
    }

    #[test]
    fn test_extract_tar_gz_identity_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.tar.gz");
        create_tar_gz_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using identity mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(identity_mapper(extract_dir.path()))?;

        // Verify extracted files
        verify_identity_extraction(extract_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_extract_tar_gz_selective_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.tar.gz");
        create_tar_gz_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using selective mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(selective_mapper(extract_dir.path()))?;

        // Only file1.txt should exist
        verify_selective_extraction(extract_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_extract_tgz_identity_mapper() -> Result<()> {
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path())?;

        let archive_dir = TempDir::new()?;
        let archive_path = archive_dir.path().join("test.tgz");
        create_tar_gz_archive(source_dir.path(), &archive_path)?;

        let extract_dir = TempDir::new()?;

        // Extract using identity mapper
        let archive_file = ArchiveFile::new(&archive_path);
        archive_file.extract(identity_mapper(extract_dir.path()))?;

        // Verify extracted files
        verify_identity_extraction(extract_dir.path())?;

        Ok(())
    }
}

#[test]
fn test_unsupported_archive_format() {
    let temp_dir = TempDir::new().unwrap();
    let archive_path = temp_dir.path().join("test.unsupported");

    // Create an empty file with unsupported extension
    File::create(&archive_path).unwrap();

    let archive_file = ArchiveFile::new(&archive_path);
    let result = archive_file.extract(|_| None);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Extract);
}
