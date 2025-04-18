//! Module for selecting the fastest mirror based on speedtest.

use std::time::Duration;

use futures_util::StreamExt;

use super::http::{Client, Response};
use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug)]
/// A struct to represent either the downloaded bytes or the time taken to download.
///
/// If the download is completed before the time limit, the value will be `Time`.
/// If the download is not completed before the time limit, the value will be `Bytes`.
///
/// When comparing two `BytesOrTime`, the `Time` is always greater than `Bytes`.
/// For two `Bytes`, the one with larger value is greater.
/// For two `Time`, the one with smaller value is greater.
enum BytesOrTime {
    Bytes(u64),
    Time(Duration),
}

// Note: we don't implement `PartialEq` for `BytesOrTime` because the order is transitive unless
// they are created with the same maximum value in speedtest.
impl BytesOrTime {
    /// # Safety
    ///
    /// Make sure that those values are generated for the same `max_bytes` and `max_time`,
    /// otherwise the result might be incorrect.
    unsafe fn gt(self, other: Self) -> bool {
        match (self, other) {
            (BytesOrTime::Bytes(a), BytesOrTime::Bytes(b)) => a > b,
            (BytesOrTime::Time(a), BytesOrTime::Time(b)) => a < b,
            (BytesOrTime::Time(_), BytesOrTime::Bytes(_)) => true,
            (BytesOrTime::Bytes(_), BytesOrTime::Time(_)) => false,
        }
    }
}

async fn speedtest(
    client: &impl Client,
    url: &str,
    max_bytes: u64,
    max_time: Duration,
) -> Result<BytesOrTime> {
    let start = std::time::Instant::now();
    let mut stream = client.get(url).await?.stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        if downloaded >= max_bytes {
            return Ok(BytesOrTime::Time(start.elapsed()));
        }
        if start.elapsed() >= max_time {
            return Ok(BytesOrTime::Bytes(downloaded));
        }
    }

    // Here, the file is fully downloaded within the time limit but not exceeding the maximum bytes.
    // This may happen that the maximum bytes are too large (larger than the file size).
    // Or the server returns a wrong file, which is not expected.
    // So, we want to return an error here, the caller should handle it.
    Err(Error::new(ErrorKind::Network).with_desc("File size exceeds maximum bytes"))
}

pub(super) async fn fastest_mirror<C, S, I>(
    client: &C,
    mirrors: I,
    max_bytes: u64,
    max_time: Duration,
) -> Option<S>
where
    C: Client,
    S: AsRef<str> + std::fmt::Display,
    I: Iterator<Item = S>,
{
    let mut fastest_mirror = None;
    let mut fastest_speed = BytesOrTime::Bytes(0);

    for mirror in mirrors {
        // Safety: Guaranteed by the caller.
        let speed = speedtest(client, mirror.as_ref(), max_bytes, max_time).await;
        log::debug!("Speedtest result for {}: {:?}", mirror, speed);
        // Do not return error if one mirror fails, just skip it
        match speed {
            Ok(speed) => {
                // Safety: Those speeds are created with the same `max_bytes` and `max_time`.
                if unsafe { speed.gt(fastest_speed) } {
                    fastest_mirror = Some(mirror);
                    fastest_speed = speed;
                }
            }
            Err(err) => {
                log::warn!("Failed to test mirror {}, reason: {}", mirror, err);
            }
        }
    }

    fastest_mirror
}

#[cfg(test)]
mod tests {
    use std::{
        pin::Pin,
        task::{Context, Poll},
        time::Duration,
    };

    use bytes::Bytes;
    use futures_util::Stream;

    use super::*;

    #[test]
    fn test_bytes_or_time_gt() {
        // Test Bytes > Bytes
        unsafe {
            assert!(BytesOrTime::Bytes(100).gt(BytesOrTime::Bytes(50)));
            assert!(!BytesOrTime::Bytes(50).gt(BytesOrTime::Bytes(100)));
            assert!(!BytesOrTime::Bytes(50).gt(BytesOrTime::Bytes(50)));
        }

        // Test Time > Time (smaller time means faster, so it's "greater")
        unsafe {
            assert!(
                BytesOrTime::Time(Duration::from_secs(1))
                    .gt(BytesOrTime::Time(Duration::from_secs(2)))
            );
            assert!(
                !BytesOrTime::Time(Duration::from_secs(2))
                    .gt(BytesOrTime::Time(Duration::from_secs(1)))
            );
            assert!(
                !BytesOrTime::Time(Duration::from_secs(1))
                    .gt(BytesOrTime::Time(Duration::from_secs(1)))
            );
        }

        // Test Time > Bytes (Time is always greater than Bytes)
        unsafe {
            assert!(BytesOrTime::Time(Duration::from_secs(10)).gt(BytesOrTime::Bytes(1000)));
            assert!(!BytesOrTime::Bytes(1000).gt(BytesOrTime::Time(Duration::from_secs(10))));
        }
    }

    use super::super::http::mock::MockClient;

    // Mock HTTP response
    #[derive(Clone)]
    struct MockResponse {
        content: Bytes,
        chunk_size: usize,
    }

    impl Response for MockResponse {
        fn stream(self) -> impl Stream<Item = Result<Bytes>> + Unpin {
            self
        }
    }

    impl Stream for MockResponse {
        type Item = Result<Bytes>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            // Check if the stream has more content to send
            if self.content.is_empty() {
                return Poll::Ready(None);
            }

            // Sleep 0.1 second to simulate network bandwidth
            std::thread::sleep(Duration::from_micros(100));
            let chunk_size = std::cmp::min(self.chunk_size, self.content.len());

            let chunk = self.content.slice(..chunk_size);

            // Update the content for next poll
            self.content = self.content.slice(chunk_size..);

            // Return the chunk
            Poll::Ready(Some(Ok(chunk)))
        }
    }

    #[tokio::test]
    async fn test_find_fastest() {
        // Create a mock client
        let mut client = MockClient::default();
        let content_size = 10000;
        let max_bytes = 3000;
        let max_time = Duration::from_secs(1);

        // Set up two mirrors with the same speed (both complete within time limit)
        let content = Bytes::from_iter(std::iter::repeat_n(0u8, content_size));
        client.add_response("http://fast.mirror.com/file", MockResponse {
            content: content.clone(),
            chunk_size: 1024, // 0.2 seconds to complete
        });
        client.add_response("http://slow.mirror.com/file", MockResponse {
            content: content.clone(),
            chunk_size: 100, //  2.0 seconds to complete
        });

        let fast_mirror_speed =
            { speedtest(&client, "http://fast.mirror.com/file", max_bytes, max_time) }
                .await
                .unwrap();
        let slow_mirror_speed =
            { speedtest(&client, "http://slow.mirror.com/file", max_bytes, max_time) }
                .await
                .unwrap();
        assert!(unsafe { fast_mirror_speed.gt(slow_mirror_speed) });

        let mirrors = &["http://fast.mirror.com/file", "http://slow.mirror.com/file"];
        let fast: &str = fastest_mirror(&client, mirrors.iter(), max_bytes, max_time)
            .await
            .unwrap();
        assert_eq!(fast, "http://fast.mirror.com/file");
    }
}
