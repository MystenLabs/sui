// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SHA_FULL_LENGTH: usize = 40;
const SHA_MIN_LENGTH: usize = 7;

pub type ShaResult<T> = std::result::Result<T, ShaError>;

#[derive(Error, Debug)]
pub enum ShaError {
    #[error("`{input}` is an invalid commit sha; commits must be between 7 and 40 characters")]
    WrongLength { input: String },

    #[error("`{input}` is an invalid commit sha; commits must be lowercase hex strings")]
    InvalidChars { input: String },
}

/// A git commit hash
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct GitSha {
    inner: String,
}

impl GitSha {
    /// Check if this is a full sha ({SHA_FULL_LENGTH} chars) or not
    pub fn is_full_sha(&self) -> bool {
        self.inner.len() == SHA_FULL_LENGTH
    }

    #[cfg(test)]
    pub fn to_short_sha(&self) -> String {
        self.inner[..7].to_string()
    }
}

impl TryFrom<String> for GitSha {
    type Error = ShaError;

    /// Check if the given string is a valid commit SHA, min 7 character, max 40 character long
    /// with only lowercase letters and digits.
    fn try_from(input: String) -> ShaResult<Self> {
        if input.len() > SHA_FULL_LENGTH || input.len() < SHA_MIN_LENGTH {
            return Err(ShaError::WrongLength { input });
        }

        if !input
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(ShaError::InvalidChars { input });
        }

        Ok(Self { inner: input })
    }
}

impl Display for GitSha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.inner.as_str())
    }
}

impl AsRef<str> for GitSha {
    fn as_ref(&self) -> &str {
        self.inner.as_ref()
    }
}

impl From<GitSha> for String {
    fn from(value: GitSha) -> Self {
        value.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::GitSha;
    use insta::assert_snapshot;

    #[test]
    fn test_git_sha() {
        let sha = "1234acb";
        assert!(GitSha::try_from(sha.to_string()).is_ok());

        let sha = "1234ac";
        assert_snapshot!(GitSha::try_from(sha.to_string()).unwrap_err().to_string(), @"`1234ac` is an invalid commit sha; commits must be between 7 and 40 characters");

        let sha = "test1234";
        assert_snapshot!(
            GitSha::try_from(sha.to_string()).unwrap_err().to_string(), @"`test1234` is an invalid commit sha; commits must be lowercase hex strings");

        let full_sha = "209f0da8e316ba6eb7310d1667bdb22ae7fcb931";
        assert!(GitSha::try_from(full_sha.to_string()).is_ok());

        let too_long_full_sha = "209f0da8e316ba6eb7310d1667bdb22ae7fcb9310";
        assert_snapshot!(GitSha::try_from(too_long_full_sha.to_string()).unwrap_err().to_string(), @"`209f0da8e316ba6eb7310d1667bdb22ae7fcb9310` is an invalid commit sha; commits must be between 7 and 40 characters");
    }
}
