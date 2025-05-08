// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tracing::debug;

use super::types::{BatchResponse, RemoteError, Request, RequestID, Response, TwoPointZero};

/// An endpoint for RPC calls.
pub struct Endpoint<I: AsyncRead, O: AsyncWrite> {
    input: BufReader<I>,
    output: O,
    sqn: RequestID,
}

#[derive(Error, Debug)]
pub enum JsonRpcError {
    #[error(transparent)]
    RemoteError(#[from] RemoteError),

    #[error(transparent)]
    IoError(#[from] tokio::io::Error),

    #[error("Received wrong set of responses")]
    IncorrectQueryResults,

    #[error("TODO: couldn't deserialize something")]
    SerializationError(#[from] serde_json::Error),
}

impl<I: AsyncRead + Unpin, O: AsyncWrite + Unpin> Endpoint<I, O> {
    /// Create an enpdoint that reads from [input] and writes to [output]
    pub fn new(input: I, output: O) -> Self {
        Self {
            input: BufReader::new(input),
            output,
            sqn: 0,
        }
    }

    /// Call the RPC method [method] with argument [arg]; decode the output
    pub async fn call<A, R>(&mut self, method: impl AsRef<str>, arg: A) -> Result<R, JsonRpcError>
    where
        A: Serialize,
        R: DeserializeOwned,
    {
        call(self, method, arg).await
    }

    /// Call the method [method] once for each argument in [args] using a JSON RPC batch request
    /// and await all of the responses. Return the results of the calls in the same order as
    /// [args].
    pub async fn batch_call<A: Serialize, R: DeserializeOwned>(
        &mut self,
        method: impl AsRef<str>,
        args: impl IntoIterator<Item = A>,
    ) -> Result<impl Iterator<Item = R>, JsonRpcError> {
        batch_call(self, method, args).await
    }
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
    let mut request_json = serde_json::to_vec(&request).expect("requests should be serializable");
    request_json.push(b'\n');

    endpoint.output.write_all(&request_json).await?;
    endpoint.output.flush().await?;

    let mut response_json = String::new();
    endpoint.input.read_line(&mut response_json).await?;

    let response: Response<R> = serde_json::de::from_str(response_json.as_str())?;

    if response.id != request.id {
        Err(JsonRpcError::IncorrectQueryResults)
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
    let requests: Vec<Request<A>> = args
        .into_iter()
        .map(|arg| make_request(endpoint, &method, arg))
        .collect();

    let mut batch_json = serde_json::to_vec(&requests).expect("requests should be serializable");
    batch_json.push(b'\n');

    endpoint.output.write_all(&batch_json).await?;

    let mut response_json = String::new();
    endpoint.input.read_line(&mut response_json).await?;

    debug!("received:{response_json}");
    let responses: BatchResponse<R> = serde_json::de::from_str(response_json.as_str())?;

    // match up requests and responses
    if responses.len() != requests.len() {
        return Err(JsonRpcError::IncorrectQueryResults);
    }

    let mut resp_by_id: BTreeMap<RequestID, Response<R>> = responses
        .into_iter()
        .map(|response| (response.id, response))
        .collect();

    let mut result: Vec<R> = Vec::new();
    for req in requests {
        let response = resp_by_id
            .remove(&req.id)
            .ok_or(JsonRpcError::IncorrectQueryResults)?;

        result.push(response.result.get::<JsonRpcError>()?);
    }

    Ok(result.into_iter())
}

/// Generate a [Request] to call [method]([arg]) using [self.sqn]; [self.sqn]
fn make_request<A: Serialize>(
    endpoint: &mut Endpoint<impl AsyncRead + Unpin, impl AsyncWrite + Unpin>,
    method: impl AsRef<str>,
    arg: A,
) -> Request<A> {
    let request = Request::<A> {
        jsonrpc: TwoPointZero,
        method: method.as_ref().to_string(),
        params: arg,
        id: endpoint.sqn,
    };
    endpoint.sqn += 1;

    request
}
