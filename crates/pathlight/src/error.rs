use core::fmt;

/// Everything that can go wrong in a pathlight call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A buffer, size or parameter does not describe a valid input.
    InvalidArgument,
    /// The font data could not be parsed.
    InvalidFont,
    /// The operation is part of the API but not implemented yet.
    Unimplemented,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::InvalidArgument => "invalid argument",
            Error::InvalidFont => "invalid font data",
            Error::Unimplemented => "not implemented yet",
        })
    }
}

impl std::error::Error for Error {}
