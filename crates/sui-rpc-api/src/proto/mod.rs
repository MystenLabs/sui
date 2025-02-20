// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod google;
pub mod node;
pub mod types;

#[cfg(test)]
mod proptests;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
pub struct TryFromProtoError {
    missing_field: Option<&'static str>,
    source: Option<BoxError>,
}

impl std::fmt::Display for TryFromProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error converting from protobuf")?;

        if let Some(missing_field) = &self.missing_field {
            write!(f, "missing_field: {missing_field}")?;
        }

        if let Some(source) = &self.source {
            write!(f, "source: {source}")?;
        }

        Ok(())
    }
}

impl std::error::Error for TryFromProtoError {}

impl TryFromProtoError {
    pub fn missing(field: &'static str) -> Self {
        Self {
            missing_field: Some(field),
            source: None,
        }
    }

    pub fn from_error<E: Into<BoxError>>(error: E) -> Self {
        Self {
            missing_field: None,
            source: Some(error.into()),
        }
    }
}

impl From<std::num::TryFromIntError> for TryFromProtoError {
    fn from(value: std::num::TryFromIntError) -> Self {
        Self::from_error(value)
    }
}

impl From<std::array::TryFromSliceError> for TryFromProtoError {
    fn from(value: std::array::TryFromSliceError) -> Self {
        Self::from_error(value)
    }
}

impl From<std::convert::Infallible> for TryFromProtoError {
    fn from(_value: std::convert::Infallible) -> Self {
        unreachable!()
    }
}
