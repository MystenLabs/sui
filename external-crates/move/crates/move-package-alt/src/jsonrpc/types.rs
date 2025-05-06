// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Implementation of the types and protocol for JSON RPC 2.0
use std::collections::BTreeMap;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{DeserializeOwned, Visitor},
};
use serde_json::json;
use thiserror::Error;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};

pub type RequestID = u64;

pub type BatchRequest<A> = Vec<Request<A>>;
pub type BatchResponse<R> = Vec<Response<R>>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Request<A> {
    pub jsonrpc: TwoPointZero,

    pub method: String,

    pub params: A,

    pub id: RequestID,
}

#[derive(Serialize, Deserialize)]
pub struct Response<R> {
    pub jsonrpc: TwoPointZero,
    pub id: RequestID,

    #[serde(flatten)]
    pub result: JsonRpcResult<R>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum JsonRpcResult<R> {
    Ok { result: R },
    Err { error: RemoteError },
}

#[derive(Serialize, Deserialize, Error, Debug, Clone, PartialEq)]
#[error("Remote endpoint returned error {code}: {message}")]
pub struct RemoteError {
    pub code: i32,
    pub message: String,

    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

impl<R> JsonRpcResult<R> {
    pub fn get<E>(self) -> Result<R, E>
    where
        E: From<RemoteError> + From<serde_json::Error>,
    {
        match self {
            JsonRpcResult::Ok { result } => Ok(result),
            JsonRpcResult::Err { error } => Err(error.into()),
        }
    }
}

/// The constant string "2.0"
#[derive(Default, Debug)]
pub struct TwoPointZero;

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
        impl serde::de::Visitor<'_> for Visitor {
            type Value = TwoPointZero;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("The string '2.0'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
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

#[test]
fn deserialize() {
    let value = json!({"result": 0});
    let x: JsonRpcResult<i32> = JsonRpcResult::deserialize(value).expect("foo");
    let JsonRpcResult::Ok { result } = x else {
        panic!()
    };
    assert_eq!(result, 0);

    let v2 = json!({"jsonrpc": "2.0", "id": 0, "result": 0});
    let response: Response<i32> = Response::deserialize(v2).expect("bar");
}
