// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SHA_FULL_LENGTH: usize = 40;

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

impl TryFrom<String> for GitSha {
    type Error = ShaError;

    /// Check if the given string is a valid commit SHA, min 7 character, max 40 character long
    /// with only lowercase letters and digits.
    fn try_from(input: String) -> ShaResult<Self> {
        if input.len() != SHA_FULL_LENGTH {
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
