// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod google;
pub mod rpc;

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

//
// TimeStamp
//

pub fn timestamp_ms_to_proto(timestamp_ms: u64) -> prost_types::Timestamp {
    let timestamp = std::time::Duration::from_millis(timestamp_ms);
    prost_types::Timestamp {
        seconds: timestamp.as_secs() as i64,
        nanos: timestamp.subsec_nanos() as i32,
    }
}

pub fn proto_to_timestamp_ms(timestamp: prost_types::Timestamp) -> Result<u64, TryFromProtoError> {
    let seconds = std::time::Duration::from_secs(timestamp.seconds.try_into()?);
    let nanos = std::time::Duration::from_nanos(timestamp.nanos.try_into()?);

    Ok((seconds + nanos).as_millis().try_into()?)
}
