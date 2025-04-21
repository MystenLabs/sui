use serde::{Deserialize, Serialize, de::Visitor};
use tokio::io::{AsyncRead, AsyncWrite};

type RpcId = String;

/// The constant "2.0"
struct JsonRpcVersion;

#[derive(Serialize, Deserialize)]
struct RpcBatchRequest<A> {
    #[serde(flatten)]
    batch: Vec<RpcRequest<A>>,
}

#[derive(Serialize, Deserialize)]
struct RpcRequest<A> {
    jsonrpc: JsonRpcVersion,
    method: String,
    params: A,
    id: RpcId,
}

#[derive(Serialize, Deserialize)]
struct RpcBatchResponse<R, E: Default> {
    #[serde(flatten)]
    batch: Vec<RpcResponse<R, E>>,
}

#[derive(Serialize, Deserialize)]
struct RpcResponse<R, E: Default> {
    jsonrpc: JsonRpcVersion,

    #[serde(flatten)]
    result: RpcResult<R, E>,

    id: RpcId,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RpcResult<R, E: Default> {
    Ok { result: R },
    Error { error: RpcError<E> },
}

#[derive(Serialize, Deserialize)]
struct RpcError<E: Default> {
    code: u64,
    message: String,

    #[serde(default)]
    data: E,
}

pub async fn rpc<A, R, E>(args: A, input: impl AsyncRead, output: impl AsyncWrite) -> Result<R, E> {
    todo!()
}

impl Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        "2.0".serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = JsonRpcVersion;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, r#"the string "2.0""#)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v != "2.0" {
                    Ok(JsonRpcVersion)
                } else {
                    Err(E::custom(r#"Expected the string "2.0""#))
                }
            }
        }
        deserializer.deserialize_string(Visitor)
    }
}
