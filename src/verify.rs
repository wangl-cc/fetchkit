use crate::error::{Error, ErrorKind, Result};

pub trait VerifierBuilder {
    type Verifier<'v>: Verifier
    where
        Self: 'v;

    fn build(&self) -> Result<Self::Verifier<'_>>;
}

const READ_BUF_SIZE: usize = 0x2000; // 8KB, which is the same as std::io::copy

/// A trait representing a verifier that can verify data
pub trait Verifier: Sized {
    /// Update the verifier with given data.
    fn update(&mut self, data: &[u8]);

    /// Update the verifier with data from a reader.
    ///
    /// # Errors
    ///
    /// If failed to read from the reader, return an error with kind `IO`.
    fn update_reader<R: std::io::Read>(&mut self, reader: &mut R) -> Result<()> {
        let mut buf = [0; READ_BUF_SIZE];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.update(&buf[..n]);
        }
        Ok(())
    }

    /// Finalize the verifier and test if the data is verified.
    ///
    /// # Errors
    ///
    /// If the data is not verified, return an error. The error should be a `Verify` error.
    /// Ideally, any other error should not be returned here, but if it is necessary, it should be
    /// any other kind of error except `Verify`.
    fn verify(self) -> Result<()>;
}

pub mod none {
    use super::*;

    /// Builder of [NoneVerifier]
    #[derive(Debug, Clone, Copy)]
    pub struct NoneVerifierBuilder;

    impl VerifierBuilder for NoneVerifierBuilder {
        type Verifier<'v>
            = NoneVerifier<'v>
        where
            Self: 'v;

        fn build(&self) -> Result<Self::Verifier<'_>> {
            Ok(NoneVerifier(std::marker::PhantomData))
        }
    }

    /// A verifier that does nothing.
    #[derive(Debug, Clone, Copy)]
    pub struct NoneVerifier<'v>(std::marker::PhantomData<&'v ()>);

    impl Verifier for NoneVerifier<'_> {
        fn update(&mut self, _data: &[u8]) {}

        fn verify(self) -> Result<()> {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_none_verifier() {
            let mut verifier = NoneVerifierBuilder.build().unwrap();
            verifier.update(b"test");
            assert!(verifier.verify().is_ok());
        }
    }
}

pub mod size {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    pub struct SizeVerifierBuilder {
        expected_size: u64,
    }

    impl SizeVerifierBuilder {
        pub fn new(expected_size: u64) -> Self {
            Self { expected_size }
        }
    }

    impl VerifierBuilder for SizeVerifierBuilder {
        type Verifier<'v>
            = SizeVerifier
        where
            Self: 'v;

        fn build(&self) -> Result<Self::Verifier<'_>> {
            Ok(SizeVerifier {
                expected_size: self.expected_size,
                current_size: 0,
            })
        }
    }

    #[derive(Debug)]
    /// The most basic verifier that checks the file size.
    ///
    /// Check the file size during download is correct. Consider using a more robust verifier like
    /// [HashVerifier] (a.k.a. checksum) or [SignatureVerifier].
    pub struct SizeVerifier {
        expected_size: u64,
        current_size: u64,
    }

    impl Verifier for SizeVerifier {
        fn update(&mut self, data: &[u8]) {
            // Check for potential overflow, although unlikely with u64
            self.current_size = self.current_size.saturating_add(data.len() as u64);
        }

        fn verify(self) -> Result<()> {
            if self.current_size == self.expected_size {
                Ok(())
            } else {
                Err(Error::new(ErrorKind::Verify).with_desc(format!(
                    "File size mismatch: expected {}, got {}",
                    self.expected_size, self.current_size
                )))
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_verify_size_correct() {
            let data = b"1234567890";
            let expected_size = 10u64;
            let builder = SizeVerifierBuilder::new(expected_size);
            let mut verifier = builder.build().unwrap();
            verifier.update(data);
            verifier.verify().expect("Verification should succeed");
        }

        #[test]
        fn test_verify_size_incorrect_too_small() {
            let data = b"12345";
            let expected_size = 10u64;
            let builder = SizeVerifierBuilder::new(expected_size);
            let mut verifier = builder.build().unwrap();
            verifier.update(data);
            let result = verifier.verify();
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Verify);
            assert!(
                err.to_string()
                    .contains("File size mismatch: expected 10, got 5")
            );
        }

        #[test]
        fn test_verify_size_incorrect_too_large() {
            let data = b"1234567890abc";
            let expected_size = 10u64;
            let builder = SizeVerifierBuilder::new(expected_size);
            let mut verifier = builder.build().unwrap();
            verifier.update(data);
            let result = verifier.verify();
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Verify);
            assert!(
                err.to_string()
                    .contains("File size mismatch: expected 10, got 13")
            );
        }

        #[test]
        fn test_verify_size_with_update_reader() {
            let expected_size = 10u64;
            let builder = SizeVerifierBuilder::new(expected_size);
            let mut verifier = builder.build().unwrap();

            // Create a cursor from the data to simulate a reader
            let data = b"1234567890";
            let mut cursor = std::io::Cursor::new(data);

            // Use update_reader to verify the data
            verifier
                .update_reader(&mut cursor)
                .expect("Failed to update from reader");
            verifier.verify().expect("Verification should succeed");

            // Test with incorrect size
            let builder = SizeVerifierBuilder::new(expected_size);
            let mut verifier = builder.build().unwrap();
            let wrong_data = vec![0; READ_BUF_SIZE + 10];
            let mut cursor = std::io::Cursor::new(wrong_data);

            verifier
                .update_reader(&mut cursor)
                .expect("Failed to update from reader");
            assert!(
                verifier.verify().is_err(),
                "Should fail with incorrect size"
            );
        }
    }
}

#[cfg(feature = "digest")]
pub mod digest {
    use ::digest::Digest;

    use super::*;

    pub struct HashVerifierBuilder<'h, D: Digest> {
        hash: &'h [u8],
        _marker: std::marker::PhantomData<D>,
    }

    impl<'h, D: Digest> HashVerifierBuilder<'h, D> {
        pub fn new(hash: &'h [u8]) -> Self {
            Self {
                hash,
                _marker: std::marker::PhantomData,
            }
        }
    }

    impl<D: Digest> VerifierBuilder for HashVerifierBuilder<'_, D> {
        type Verifier<'v>
            = HashVerifier<'v, D>
        where
            Self: 'v;

        fn build(&self) -> Result<Self::Verifier<'_>> {
            let hash = self.hash;
            if hash.len() != <D as Digest>::output_size() {
                return Err(crate::error::Error::new(crate::error::ErrorKind::Verify)
                    .with_desc("Invalid hash length"));
            }
            Ok(HashVerifier {
                hasher: D::new(),
                hash,
            })
        }
    }

    pub struct HashVerifier<'h, D: Digest> {
        hasher: D,
        hash: &'h [u8],
    }

    impl<D: Digest> Verifier for HashVerifier<'_, D> {
        fn update(&mut self, data: &[u8]) {
            self.hasher.update(data);
        }

        fn verify(self) -> Result<()> {
            let digest = self.hasher.finalize();
            if digest.as_slice() == self.hash {
                Ok(())
            } else {
                Err(Error::new(ErrorKind::Verify).with_desc("Hash mismatch"))
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use sha2::Sha256;

        use super::*;

        // This is the hash of "hello world\n" calculated by sha256 in binary mode
        #[rustfmt::skip]
        static HASH: &[u8; 32] = &[
            0xa9, 0x48, 0x90, 0x4f, 0x2f, 0x0f, 0x47, 0x9b,
            0x8f, 0x81, 0x97, 0x69, 0x4b, 0x30, 0x18, 0x4b,
            0x0d, 0x2e, 0xd1, 0xc1, 0xcd, 0x2a, 0x1e, 0xc0,
            0xfb, 0x85, 0xd2, 0x99, 0xa1, 0x92, 0xa4, 0x47,
        ];

        #[test]
        fn test_invalid_hash() {
            let builder = HashVerifierBuilder::<Sha256>::new(&[0, 1, 2, 3]);
            assert!(builder.build().is_err(), "Should fail with invalid hash");
        }

        #[test]
        fn test_verify_hash() {
            let builder = HashVerifierBuilder::<Sha256>::new(HASH);
            let mut verifier = builder.build().unwrap();

            verifier.update(b"hello world\n");
            verifier.verify().expect("Failed to verify hash");
        }

        #[test]
        fn test_verify_hash_mismatch() {
            let builder = HashVerifierBuilder::<Sha256>::new(HASH);
            let mut verifier = builder.build().unwrap();

            verifier.update(b"hello false hash\n");
            let result = verifier.verify();

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Verify);
            assert!(err.to_string().contains("Hash mismatch"));
        }
    }
}

#[cfg(feature = "minisign")]
pub mod minisign {
    use minisign_verify::{PublicKey, Signature, StreamVerifier};

    use super::{Error, Result, Verifier, VerifierBuilder};

    pub struct MinisignVerifierBuilder<'v> {
        key: &'v PublicKey,
        signature: &'v Signature,
    }

    impl<'v> MinisignVerifierBuilder<'v> {
        pub fn new(key: &'v PublicKey, signature: &'v Signature) -> Self {
            Self { key, signature }
        }
    }

    impl VerifierBuilder for MinisignVerifierBuilder<'_> {
        type Verifier<'v>
            = MinisignVerifier<'v>
        where
            Self: 'v;

        fn build(&self) -> Result<Self::Verifier<'_>> {
            match self.key.verify_stream(self.signature) {
                Ok(verifier) => Ok(MinisignVerifier(verifier)),
                Err(e) => Err(Error::new(crate::error::ErrorKind::Verify).with_source(e)),
            }
        }
    }

    pub struct MinisignVerifier<'v>(StreamVerifier<'v>);

    impl Verifier for MinisignVerifier<'_> {
        fn update(&mut self, data: &[u8]) {
            self.0.update(data);
        }

        fn verify(mut self) -> Result<()> {
            self.0
                .finalize()
                .map_err(|e| Error::new(crate::error::ErrorKind::Verify).with_source(e))
        }
    }

    #[cfg(test)]
    mod tests {
        use minisign_verify::{PublicKey, Signature};

        use super::*;

        static KEY_B64: &str = "RWSj7AAKARfXiSiVLt+Nd3NVHliXzb+P+RYG49exdGIpiIoms7gWjVSo";

        // Test to verify a minisign signature, which is created by minisign cli
        #[test]
        fn test_verify() {
            let key = PublicKey::from_base64(KEY_B64).expect("Failed to decode public key");

            let signature_text = "untrusted comment: test sign\n\
            RUSj7AAKARfXiTQqJYgBoHpGGY08jnWgP1qLrKD5T6DnsTjgvveat3JIfxsP9pemxkbvn4EusnNib4v5iktxgv3vEdoQblx/qAQ=\n\
            trusted comment: sign for hello world\n\
            U6AtSJi5CUgMXgnhNmPDkgw4hjzo7y3u20cw0psAzVCkms+I2vStsxlmZGz/udIPMtW1DDBASz9cezsVaSWxDg==\n";
            let signature = Signature::decode(signature_text).expect("Failed to decode signature");

            let builder = MinisignVerifierBuilder::new(&key, &signature);

            let mut verifier = builder.build().expect("Failed to create verifier");

            verifier.update(b"hello world\n");

            verifier.verify().expect("Failed to verify signature");
        }

        #[test]
        fn test_verify_invalid_signature() {
            let encoded_key = "RWSj7AAKARfXiSiVLt+Nd3NVHliXzb+P+RYG49exdGIpiIoms7gWjVSo";
            let key = PublicKey::from_base64(encoded_key).expect("Failed to decode public key");

            let signature_text = "untrusted comment: test sign\n\
            RUSj7AAKARfXiTQqJYgBoHpGGY08jnWgP1qLrKD5T6DnsTjgvveat3JIfxsP9pemxkbvn4EusnNib4v5iktxgv3vEdoQblx/qAQ=\n\
            trusted comment: sign for hello world with wrong trust comment\n\
            U6AtSJi5CUgMXgnhNmPDkgw4hjzo7y3u20cw0psAzVCkms+I2vStsxlmZGz/udIPMtW1DDBASz9cezsVaSWxDg==\n";
            let signature = Signature::decode(signature_text).expect("Failed to decode signature");

            let builder = MinisignVerifierBuilder::new(&key, &signature);

            let mut verifier = builder.build().expect("Failed to create verifier");

            verifier.update(b"hello world\n");

            assert!(verifier.verify().is_err());
        }
    }
}
