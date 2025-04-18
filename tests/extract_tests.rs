use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use fetchkit::{
    error::{Error, ErrorKind, Result},
    extract::{Archive, ArchiveFile},
};
use tempfile::TempDir;

// Helper function to create a directory structure with files for testing
fn create_test_files(dir: &Path) -> Result<()> {
    // Create a simple directory structure
    let subdir_path = dir.join("subdir");
    fs::create_dir_all(&subdir_path)?;

    // Create a file in the root directory
    let mut file1 = File::create(dir.join("file1.txt"))?;
    file1.write_all(b"This is file 1")?;

    // Create a file in the subdirectory
    let mut file2 = File::create(subdir_path.join("file2.txt"))?;
    file2.write_all(b"This is file 2")?;

    Ok(())
}

// Helper function to verify extracted files with identity mapper (all files should be extracted)
fn verify_identity_extraction(extract_dir: &Path) -> Result<()> {
    let file1_path = extract_dir.join("file1.txt");
    let file2_path = extract_dir.join("subdir/file2.txt");

    // Check that files exist
    assert!(file1_path.exists(), "file1.txt should exist");
    assert!(file2_path.exists(), "file2.txt should exist");

    // Check file contents
    let mut content = String::new();
    File::open(file1_path)?.read_to_string(&mut content)?;
    assert_eq!(content, "This is file 1", "file1.txt content mismatch");

    content.clear();
    File::open(file2_path)?.read_to_string(&mut content)?;
    assert_eq!(content, "This is file 2", "file2.txt content mismatch");

    Ok(())
}

// Helper function to verify extracted files with selective mapper (only file1.txt should be
// extracted)
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

// Helper function to create the identity mapper
fn identity_mapper(output_dir: &Path) -> impl FnMut(&Path) -> Option<PathBuf> + '_ {
    move |path: &Path| Some(output_dir.join(path))
}

// Helper function to create a selective mapper that only extracts certain files
fn selective_mapper(output_dir: &Path) -> impl FnMut(&Path) -> Option<PathBuf> + '_ {
    move |path: &Path| {
        if path.to_string_lossy().ends_with("file1.txt") {
            Some(output_dir.join(path))
        } else {
            None
        }
    }
}

#[cfg(feature = "zip")]
mod zip_tests {
    use super::*;

    fn create_zip_archive(source_dir: &Path, archive_path: &Path) -> Result<()> {
        // Create a command to zip the directory
        let status = std::process::Command::new("zip")
            .arg("-r")
            .arg(archive_path)
            .arg(".")
            .current_dir(source_dir)
            .status()
            .expect("Failed to execute zip command");

        if !status.success() {
            return Err(Error::new(ErrorKind::Extract).with_desc("Failed to create zip archive"));
        }

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
