//! This module defines a rudimentary interface for JSON RPC 2.0 clients. The current
//! implementation requires the remote endpoint to send responses in the same order as
//! requests are written (subrequests of a batch request can be returned in any order).

// TODO: this lives here because it supports external resolvers, but it is completely independent
// and should maybe be made into its own crate?

use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};

type RequestID = u64;

/// An endpoint for RPC calls.
pub struct Endpoint<I: AsyncRead, O: AsyncWrite> {
    input: BufReader<I>,
    output: BufWriter<O>,
    sqn: RequestID,
}

#[derive(Serialize, Deserialize, Error, Debug)]
#[error("Remote endpoint returned error {code}: {message}")]
pub struct RemoteError {
    code: i32,
    message: String,

    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(Error, Debug)]
pub enum JsonRpcError {
    #[error(transparent)]
    RemoteError(#[from] RemoteError),

    #[error(transparent)]
    IoError(#[from] tokio::io::Error),

    #[error("Received responses in the wrong order")]
    OutOfOrder,

    #[error("Received wrong set of responses")]
    WrongResponses,

    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
}

impl<I: AsyncRead + Unpin, O: AsyncWrite + Unpin> Endpoint<I, O> {
    /// Create an enpdoint that reads from [input] and writes to [output]
    pub fn new(input: I, output: O) -> Self {
        Self {
            input: BufReader::new(input),
            output: BufWriter::new(output),
            sqn: 0,
        }
    }

    /// Call the RPC method [method] with argument [arg]; decode the output
    pub async fn call<A, R>(&mut self, method: impl AsRef<str>, arg: A) -> Result<R, JsonRpcError>
    where
        A: Serialize,
        R: DeserializeOwned,
    {
        rpc_impl::call(self, method, arg).await
    }

    /// Call the method [method] once for each argument in [args] using a JSON RPC batch request
    /// and await all of the responses. Return the results of the calls in the same order as
    /// [args].
    pub async fn batch_call<A: Serialize, R: DeserializeOwned>(
        &mut self,
        method: impl AsRef<str>,
        args: impl IntoIterator<Item = A>,
    ) -> Result<impl Iterator<Item = R>, JsonRpcError> {
        rpc_impl::batch_call(self, method, args).await
    }
}

/// Implementation of the types and protocol for JSON RPC 2.0
mod rpc_impl {
    use std::collections::BTreeMap;

    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use tokio::io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
    };

    use super::{Endpoint, JsonRpcError, RemoteError, RequestID};

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "", deny_unknown_fields)]
    struct BatchRequest {
        #[serde(flatten)]
        pub requests: Vec<Request>,
    }

    /// The constant string "2.0"
    #[derive(Default, Debug)]
    struct TwoPointZero;

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "", deny_unknown_fields)]
    struct BatchResponse {
        #[serde(flatten)]
        pub responses: Vec<Response>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct Request {
        jsonrpc: TwoPointZero,

        method: String,

        params: serde_json::Value,

        id: RequestID,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        jsonrpc: TwoPointZero,
        id: RequestID,

        #[serde(flatten)]
        result: JsonRpcResult,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum JsonRpcResult {
        Ok { result: serde_json::Value },
        Err { error: RemoteError },
    }

    /// Call the RPC method [method] with argument [arg]; decode the output
    pub async fn call<A, R, I: AsyncRead + Unpin, O: AsyncWrite + Unpin>(
        endpoint: &mut Endpoint<I, O>,
        method: impl AsRef<str>,
        arg: A,
    ) -> Result<R, JsonRpcError>
    where
        A: Serialize,
        R: DeserializeOwned,
    {
        let request = make_request(endpoint, method, arg);
        let request_json = serde_json::to_vec(&request).expect("requests should be serializable");

        endpoint.output.write_all(&request_json).await?;

        let mut response_json = String::new();
        endpoint.input.read_line(&mut response_json).await?;

        let response: Response = serde_json::de::from_str(response_json.as_str())?;

        if response.id != request.id {
            Err(JsonRpcError::OutOfOrder)
        } else {
            response.result.get()
        }
    }

    /// Call the method [method] once for each argument in [args] using a JSON RPC batch request
    /// and await all of the responses. Return the results of the calls in the same order as
    /// [args].
    pub async fn batch_call<A: Serialize, R: DeserializeOwned>(
        endpoint: &mut Endpoint<impl AsyncRead + Unpin, impl AsyncWrite + Unpin>,
        method: impl AsRef<str>,
        args: impl IntoIterator<Item = A>,
    ) -> Result<impl Iterator<Item = R>, JsonRpcError> {
        let requests: Vec<Request> = args
            .into_iter()
            .map(|arg| make_request(endpoint, &method, arg))
            .collect();

        let batch = BatchRequest { requests };

        let batch_json = serde_json::to_vec(&batch).expect("requests should be serializable");

        endpoint.output.write_all(&batch_json).await?;

        let mut response_json = String::new();
        endpoint.input.read_line(&mut response_json).await?;

        let responses: BatchResponse = serde_json::de::from_str(response_json.as_str())?;

        // match up requests and responses
        if responses.responses.len() != batch.requests.len() {
            return Err(JsonRpcError::WrongResponses);
        }

        let mut resp_by_id: BTreeMap<RequestID, Response> = responses
            .responses
            .into_iter()
            .map(|response| (response.id, response))
            .collect();

        let mut result: Vec<R> = Vec::new();
        for req in batch.requests {
            let response = resp_by_id
                .remove(&req.id)
                .ok_or_else(|| JsonRpcError::OutOfOrder)?;

            result.push(response.result.get()?);
        }

        Ok(result.into_iter())
    }

    /// Generate a [Request] to call [method]([arg]) using [self.sqn]; [self.sqn]
    fn make_request<A: Serialize>(
        endpoint: &mut Endpoint<impl AsyncRead + Unpin, impl AsyncWrite + Unpin>,
        method: impl AsRef<str>,
        arg: A,
    ) -> Request {
        let request = Request {
            jsonrpc: TwoPointZero,
            method: method.as_ref().to_string(),
            params: serde_json::to_value(arg).expect("arguments should be serializable"),
            id: endpoint.sqn,
        };
        endpoint.sqn += 1;

        request
    }

    impl JsonRpcResult {
        fn get<R: DeserializeOwned>(self) -> Result<R, JsonRpcError> {
            match self {
                JsonRpcResult::Ok { result } => Ok(R::deserialize(result)?),
                JsonRpcResult::Err { error } => Err(error.into()),
            }
        }
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
}

#[cfg(test)]
mod test {
    // TODO
}
