#[derive(Debug, Clone, Copy)]
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
    ///
    /// Note: `Other` error is not equal to self.
    Other,
}

impl PartialEq for ErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Network, Self::Network) => true,
            (Self::Io, Self::Io) => true,
            (Self::Verify, Self::Verify) => true,
            (Self::Extract, Self::Extract) => true,
            // Other might be any other error kind, so we don't treat it as equal
            (Self::Other, Self::Other) => false,
            _ => false,
        }
    }
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
