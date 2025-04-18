//! Trait for HTTP client and response used by this crate
//!
//! This makes it possible to use different HTTP clients in the same codebase,
//! and use a mock client for testing.

use bytes::Bytes;
use futures_util::Stream;

use crate::error::Error;

/// A trait representing a HTTP client
pub trait Client {
    type Response: Response;

    /// Send a GET request to the specified URL.
    fn get(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = Result<Self::Response, Error>> + Send;
}

/// A trait representing a HTTP response
pub trait Response {
    /// Consumes the response and returns a stream of bytes.
    fn stream(self) -> impl Stream<Item = Result<Bytes, Error>> + Unpin;
}

#[cfg(feature = "reqwest")]
mod reqwest {
    use futures_util::StreamExt;

    use super::*;

    impl Client for ::reqwest::Client {
        type Response = ::reqwest::Response;

        async fn get(&self, url: &str) -> Result<Self::Response, Error> {
            Ok(self.get(url).send().await?)
        }
    }

    impl Response for ::reqwest::Response {
        fn stream(self) -> impl Stream<Item = Result<Bytes, Error>> + Unpin {
            self.bytes_stream().map(|result| result.map_err(Into::into))
        }
    }

    impl From<::reqwest::Error> for Error {
        fn from(error: ::reqwest::Error) -> Self {
            Self::new(crate::error::ErrorKind::Network).with_source(error)
        }
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use crate::error::ErrorKind;

    // Mock HTTP client
    pub struct MockClient<R> {
        responses: std::collections::HashMap<String, R>,
    }

    impl<R> Default for MockClient<R> {
        fn default() -> Self {
            Self {
                responses: Default::default(),
            }
        }
    }

    impl<R: Response> MockClient<R> {
        pub fn add_response(&mut self, url: &str, response: R) {
            self.responses.insert(url.to_string(), response);
        }
    }

    impl<R: Response + Clone + Sync> Client for MockClient<R> {
        type Response = R;

        async fn get(&self, url: &str) -> Result<Self::Response, Error> {
            match self.responses.get(url) {
                Some(response) => Ok(response.clone()),
                None => {
                    Err(Error::new(ErrorKind::Network).with_desc("URL not found in mock responses"))
                }
            }
        }
    }
}
