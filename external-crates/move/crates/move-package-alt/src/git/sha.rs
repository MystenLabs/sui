// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Deserializer, Serialize};

use super::errors::{ShaError, ShaResult};

/// A git commit hash
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct GitSha {
    inner: String,
}

impl TryFrom<String> for GitSha {
    type Error = ShaError;

    /// Check if the given string is a valid commit SHA, i.e., 40 character long with only
    /// lowercase letters and digits
    fn try_from(input: String) -> ShaResult<Self> {
        if input.len() != 40 {
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

impl AsRef<str> for &GitSha {
    fn as_ref(&self) -> &str {
        self.inner.as_ref()
    }
}

impl From<GitSha> for String {
    fn from(value: GitSha) -> Self {
        value.inner
    }
}
