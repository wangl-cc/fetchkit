mod mirror;

pub mod http;

use std::{io::Write, path::Path, time::Duration};

use futures_util::StreamExt;
use http::Response;

use crate::{
    error::{Error, ErrorKind, Result},
    progress::{ProgressReceiver, ProgressReceiverBuilder},
    verify::{Verifier, VerifierBuilder, none::NoneVerifierBuilder},
};

pub struct DownloadBuilder<'m, V = NoneVerifierBuilder> {
    url: &'m str,
    dest: &'m Path,
    size: u64,
    verifier: Option<V>,
    mirror_options: Option<MirrorOptions<'m>>,
}

impl<'a, V> DownloadBuilder<'a, V>
where
    V: VerifierBuilder + 'a,
{
    pub fn new(url: &'a str, dest: &'a Path, size: u64) -> Self {
        Self {
            url,
            dest,
            size,
            verifier: None,
            mirror_options: None,
        }
    }

    pub fn with_verifier(mut self, verifier: V) -> Self {
        self.verifier = Some(verifier);
        self
    }

    pub fn with_mirror_options(mut self, options: MirrorOptions<'a>) -> Self {
        self.mirror_options = Some(options);
        self
    }

    /// Check if the destination file exists and is valid.
    ///
    /// This function is useful when you want to check if the file is already downloaded.
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails to open the destination due to permission or
    /// other io related errors.
    pub fn exist(&self) -> Result<bool> {
        if self.dest.exists() {
            let mut file = std::fs::File::open(self.dest)?;
            if file.metadata()?.len() != self.size {
                return Ok(false);
            }
            if let Some(verifier) = &self.verifier {
                return verifier.build()?.update_reader(&mut file).map(|_| true);
            }

            return Ok(true);
        }

        Ok(false)
    }

    /// Download file from the given url(s) with the given http client.
    pub async fn download(
        self,
        client: &impl http::Client,
        progress: Option<impl ProgressReceiverBuilder>,
    ) -> Result<()> {
        let url = if let Some(opts) = self.mirror_options {
            let mirrors = std::iter::once(self.url).chain(opts.mirrors.iter().copied());
            mirror::fastest_mirror(client, mirrors, opts.max_bytes, opts.max_time)
                .await
                .ok_or(Error::new(ErrorKind::Network).with_desc("No mirrors available"))?
        } else {
            self.url
        };

        let mut verifier = self
            .verifier
            .as_ref()
            .map(|verifier| verifier.build())
            .transpose()?;

        let progress = progress.map(|p| p.init(self.size));

        let resp = client.get(url).await?;

        let mut write = std::fs::File::create_new(self.dest)?;

        let mut stream = resp.stream();
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            write.write_all(&chunk)?;
            if let Some(progress) = &progress {
                progress.set_position(downloaded);
            }
            if let Some(verifier) = &mut verifier {
                verifier.update(&chunk);
            }
        }
        if let Some(progress) = progress {
            progress.finish();
        }
        if let Some(verifier) = verifier {
            verifier.verify()?;
        }

        Ok(())
    }
}

// TODO: move this to mirror.rs and move mirror test into method of this struct
pub struct MirrorOptions<'m> {
    mirrors: &'m [&'m str],
    max_bytes: u64,
    max_time: Duration,
    error_handler: Option<Box<dyn FnMut(Error)>>,
}

impl<'m> MirrorOptions<'m> {
    pub fn new(mirrors: &'m [&'m str], max_bytes: u64, max_time: Duration) -> Self {
        Self {
            mirrors,
            max_bytes,
            max_time,
            error_handler: None,
        }
    }

    /// Register an error handler when an error occurs during testing mirrors.
    pub fn with_error_handler(mut self, handler: Box<dyn FnMut(Error)>) -> Self {
        self.error_handler = Some(handler);
        self
    }
}
