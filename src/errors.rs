use core::fmt::Formatter;

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Error {
    TooFewShards,
    TooManyShards,
    TooFewDataShards,
    TooManyDataShards,
    TooFewParityShards,
    TooManyParityShards,
    TooFewBufferShards,
    TooManyBufferShards,
    IncorrectShardSize,
    TooFewShardsPresent,
    EmptyShard,
    InvalidShardFlags,
    InvalidIndex,
    InvalidCustomMatrix,
    UnsupportedCodecFamily,
    UnsupportedLeopardPrototype,
}

impl Error {
    fn as_str(self) -> &'static str {
        match self {
            Error::TooFewShards => "The number of provided shards is smaller than the one in codec",
            Error::TooManyShards => {
                "The number of provided shards is greater than the one in codec"
            }
            Error::TooFewDataShards => {
                "The number of provided data shards is smaller than the one in codec"
            }
            Error::TooManyDataShards => {
                "The number of provided data shards is greater than the one in codec"
            }
            Error::TooFewParityShards => {
                "The number of provided parity shards is smaller than the one in codec"
            }
            Error::TooManyParityShards => {
                "The number of provided parity shards is greater than the one in codec"
            }
            Error::TooFewBufferShards => {
                "The number of provided buffer shards is smaller than the number of parity shards in codec"
            }
            Error::TooManyBufferShards => {
                "The number of provided buffer shards is greater than the number of parity shards in codec"
            }
            Error::IncorrectShardSize => {
                "At least one of the provided shards is not of the correct size"
            }
            Error::TooFewShardsPresent => {
                "The number of shards present is smaller than number of parity shards, cannot reconstruct missing shards"
            }
            Error::EmptyShard => "The first shard provided is of zero length",
            Error::InvalidShardFlags => {
                "The number of flags does not match the total number of shards"
            }
            Error::InvalidIndex => {
                "The data shard index provided is greater or equal to the number of data shards in codec"
            }
            Error::InvalidCustomMatrix => {
                "The encoding matrix is invalid, missing, or incompatible with the reconstruction workspace"
            }
            Error::UnsupportedCodecFamily => {
                "The selected codec family is not supported for this field or configuration"
            }
            Error::UnsupportedLeopardPrototype => {
                "The selected Leopard codec family is only available as a prototype skeleton in this build"
            }
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.as_str()
    }
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum SBSError {
    TooManyCalls,
    LeftoverShards,
    RSError(Error),
}

impl SBSError {
    fn as_str(self) -> &'static str {
        match self {
            SBSError::TooManyCalls => "Too many calls",
            SBSError::LeftoverShards => "Leftover shards",
            SBSError::RSError(ref e) => e.as_str(),
        }
    }
}

impl core::fmt::Display for SBSError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SBSError {
    fn description(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use crate::errors::Error;
    use crate::errors::SBSError;

    #[test]
    fn test_error_to_string_is_okay() {
        assert_eq!(
            alloc::format!("{}", Error::TooFewShards),
            "The number of provided shards is smaller than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooManyShards),
            "The number of provided shards is greater than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooFewDataShards),
            "The number of provided data shards is smaller than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooManyDataShards),
            "The number of provided data shards is greater than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooFewParityShards),
            "The number of provided parity shards is smaller than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooManyParityShards),
            "The number of provided parity shards is greater than the one in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooFewBufferShards),
            "The number of provided buffer shards is smaller than the number of parity shards in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooManyBufferShards),
            "The number of provided buffer shards is greater than the number of parity shards in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::IncorrectShardSize),
            "At least one of the provided shards is not of the correct size"
        );
        assert_eq!(
            alloc::format!("{}", Error::TooFewShardsPresent),
            "The number of shards present is smaller than number of parity shards, cannot reconstruct missing shards"
        );
        assert_eq!(
            alloc::format!("{}", Error::EmptyShard),
            "The first shard provided is of zero length"
        );
        assert_eq!(
            alloc::format!("{}", Error::InvalidShardFlags),
            "The number of flags does not match the total number of shards"
        );
        assert_eq!(
            alloc::format!("{}", Error::InvalidIndex),
            "The data shard index provided is greater or equal to the number of data shards in codec"
        );
        assert_eq!(
            alloc::format!("{}", Error::InvalidCustomMatrix),
            "The encoding matrix is invalid, missing, or incompatible with the reconstruction workspace"
        );
        assert_eq!(
            alloc::format!("{}", Error::UnsupportedCodecFamily),
            "The selected codec family is not supported for this field or configuration"
        );
        assert_eq!(
            alloc::format!("{}", Error::UnsupportedLeopardPrototype),
            "The selected Leopard codec family is only available as a prototype skeleton in this build"
        );
    }

    #[test]
    fn test_sbserror_to_string_is_okay() {
        assert_eq!(
            alloc::format!("{}", SBSError::TooManyCalls),
            "Too many calls"
        );
        assert_eq!(
            alloc::format!("{}", SBSError::LeftoverShards),
            "Leftover shards"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_error_display_does_not_panic() {
        println!("{}", Error::TooFewShards);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_sbserror_display_does_not_panic() {
        println!("{}", SBSError::TooManyCalls);
    }
}
