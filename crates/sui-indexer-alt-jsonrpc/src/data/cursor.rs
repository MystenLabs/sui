// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CursorError {
    #[error("Failed to serialize cursor: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Failed to decode base64 cursor: {0}")]
    Base64DecodeError(#[from] base64::DecodeError),
}

/// An opaque cursor type that can be serialized to/from a base64 string for use in JSON RPC pagination.
/// The inner type T must implement Serialize and Deserialize.
#[derive(Debug, Clone)]
pub struct JsonCursor<T>(T);

impl<T> JsonCursor<T> {
    /// Create a new cursor from the inner value
    pub fn new(inner: T) -> Self {
        Self(inner)
    }

    /// Get a reference to the inner value
    pub fn inner(&self) -> &T {
        &self.0
    }

    /// Convert the cursor into its inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Serialize> JsonCursor<T> {
    /// Encode the cursor as a base64 string
    fn to_base64(&self) -> Result<String, CursorError> {
        let json = serde_json::to_string(&self.0)?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }
}

impl<T: DeserializeOwned> JsonCursor<T> {
    /// Decode a cursor from a base64 string
    fn from_base64(s: &str) -> Result<Self, CursorError> {
        let bytes = URL_SAFE_NO_PAD.decode(s)?;
        let inner = serde_json::from_slice(&bytes)?;
        Ok(Self(inner))
    }
}

// Custom serialization to always encode as string
impl<T: Serialize> Serialize for JsonCursor<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_base64()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

// Custom deserialization from string
impl<'de, T: DeserializeOwned> Deserialize<'de> for JsonCursor<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_base64(&s).map_err(serde::de::Error::custom)
    }
}

impl<T> JsonSchema for JsonCursor<T> {
    fn schema_name() -> String {
        "Cursor".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }

    fn is_referenceable() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    struct TestCursor {
        page: u32,
        offset: u64,
    }

    #[test]
    fn test_cursor_encode_decode() {
        let inner = TestCursor {
            page: 1,
            offset: 100,
        };
        let cursor = JsonCursor::new(inner.clone());

        // Test JSON serialization
        let json_value = serde_json::to_value(&cursor).unwrap();
        assert!(json_value.is_string());

        // Test JSON deserialization
        let decoded: JsonCursor<TestCursor> = serde_json::from_value(json_value).unwrap();
        assert_eq!(decoded.inner(), &inner);
    }

    #[test]
    fn test_cursor_in_json_params() {
        let inner = TestCursor {
            page: 1,
            offset: 100,
        };
        let cursor = JsonCursor::new(inner);

        // Test as part of JSON-RPC params
        let params = json!({
            "cursor": cursor,
            "limit": 50
        });

        assert!(params["cursor"].is_string());

        // Should be able to deserialize from the params
        let _: JsonCursor<TestCursor> = serde_json::from_value(params["cursor"].clone()).unwrap();
    }
}
