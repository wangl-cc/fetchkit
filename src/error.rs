#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// I/O error
    Io,
    /// Verification error
    Verify,
    /// Extraction error
    Extract,
    /// Network error
    Network,
    /// Any other error not listed above
    Other,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Network => f.write_str("Network error"),
            Self::Io => f.write_str("I/O error"),
            Self::Verify => f.write_str("Verification error"),
            Self::Extract => f.write_str("Extraction error"),
            Self::Other => f.write_str("Other error"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub struct Error {
    kind: ErrorKind,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    description: Option<std::borrow::Cow<'static, str>>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(description) = &self.description {
            write!(f, ": {}", description)?;
        }
        Ok(())
    }
}

impl Error {
    pub const fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            source: None,
            description: None,
        }
    }

    pub fn with_source(
        mut self,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_desc(mut self, desc: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io).with_source(error)
    }
}

/// A convenience trait for appending descriptions to errors.
pub trait WithDesc<T> {
    /// Append a description if there is an error.
    fn with_desc(self, desc: &'static str) -> Result<T>;

    /// Lazily append a description if there is an error.
    fn then_with_desc(self, f: impl FnOnce() -> String) -> Result<T>;
}

impl<T, E: Into<Error>> WithDesc<T> for std::result::Result<T, E> {
    fn with_desc(self, desc: &'static str) -> Result<T> {
        self.map_err(|err| Error::with_desc(err.into(), desc))
    }

    fn then_with_desc(self, f: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|err| Error::with_desc(err.into(), f()))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// module
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_kind_display() {
        assert_eq!(format!("{}", ErrorKind::Io), "I/O error");
        assert_eq!(format!("{}", ErrorKind::Verify), "Verification error");
        assert_eq!(format!("{}", ErrorKind::Extract), "Extraction error");
        assert_eq!(format!("{}", ErrorKind::Network), "Network error");
        assert_eq!(format!("{}", ErrorKind::Other), "Other error");
    }

    #[test]
    fn test_error_display() {
        let error = Error::new(ErrorKind::Io);
        assert_eq!(format!("{}", error), "I/O error");

        let error = Error::new(ErrorKind::Network).with_desc("failed to connect");
        assert_eq!(format!("{}", error), "Network error: failed to connect");
    }

    #[test]
    fn test_error_kind_partial_eq() {
        assert_eq!(ErrorKind::Io, ErrorKind::Io);
        assert_eq!(ErrorKind::Verify, ErrorKind::Verify);
        assert_eq!(ErrorKind::Extract, ErrorKind::Extract);
        assert_eq!(ErrorKind::Network, ErrorKind::Network);
        assert_eq!(ErrorKind::Other, ErrorKind::Other);

        assert_ne!(ErrorKind::Io, ErrorKind::Network);
        assert_ne!(ErrorKind::Verify, ErrorKind::Extract);
        assert_ne!(ErrorKind::Other, ErrorKind::Io);
    }

    #[test]
    fn test_error_creation() {
        let error = Error::new(ErrorKind::Io);
        assert_eq!(error.kind(), ErrorKind::Io);
        assert_eq!(error.description(), None);

        let error = Error::new(ErrorKind::Network).with_desc("failed to connect");
        assert_eq!(error.kind(), ErrorKind::Network);
        assert_eq!(error.description(), Some("failed to connect"));

        let error = Error::new(ErrorKind::Other).with_desc("failed to other");
        assert_eq!(error.description(), Some("failed to other"));
    }

    #[test]
    fn test_error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error = Error::new(ErrorKind::Io).with_source(io_err);
        assert_eq!(error.kind(), ErrorKind::Io);
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: Error = io_err.into();
        assert_eq!(error.kind(), ErrorKind::Io);
    }

    #[test]
    fn test_with_desc() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.with_desc("this won't be used").unwrap(), 42);
        let result: Result<i32> = Ok(42);
        assert_eq!(
            result
                .then_with_desc(|| "this won't be used".to_string())
                .unwrap(),
            42
        );

        let result: std::result::Result<i32, Error> = Err(Error::new(ErrorKind::Network));
        let err = result.with_desc("connection failed").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Network);
        assert_eq!(err.description(), Some("connection failed"));

        let result: std::result::Result<i32, Error> = Err(Error::new(ErrorKind::Verify));
        let err = result
            .then_with_desc(|| "verification failed".to_string())
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Verify);
        assert_eq!(err.description(), Some("verification failed"));
    }
}
