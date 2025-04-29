/// Implementation of the types and protocol for JSON RPC 2.0
use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};

pub type RequestID = u64;

#[derive(Serialize, Deserialize)]
#[serde(bound = "", deny_unknown_fields)]
pub struct BatchRequest {
    #[serde(flatten)]
    pub requests: Vec<Request>,
}

/// The constant string "2.0"
#[derive(Default, Debug)]
pub struct TwoPointZero;

#[derive(Serialize, Deserialize)]
#[serde(bound = "", deny_unknown_fields)]
pub struct BatchResponse {
    #[serde(flatten)]
    pub responses: Vec<Response>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: TwoPointZero,

    pub method: String,

    pub params: serde_json::Value,

    pub id: RequestID,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub jsonrpc: TwoPointZero,
    pub id: RequestID,

    pub result: JsonRpcResult,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResult {
    Ok { result: serde_json::Value },
    Err { error: RemoteError },
}

impl JsonRpcResult {
    pub fn get<R: DeserializeOwned, E: From<RemoteError> + From<serde_json::Error>>(
        self,
    ) -> Result<R, E> {
        match self {
            JsonRpcResult::Ok { result } => Ok(R::deserialize(result)?),
            JsonRpcResult::Err { error } => Err(error.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Error, Debug)]
#[error("Remote endpoint returned error {code}: {message}")]
pub struct RemoteError {
    pub code: i32,
    pub message: String,

    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

impl Serialize for TwoPointZero {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for TwoPointZero {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TwoPointZero;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("The string '2.0'")
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v != "2.0" {
                    Err(E::custom("The string is not '2.0'"))
                } else {
                    Ok(TwoPointZero)
                }
            }
        }

        deserializer.deserialize_string(Visitor)
    }
}
