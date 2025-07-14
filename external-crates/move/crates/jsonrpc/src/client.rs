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

    #[error("received wrong set of responses")]
    IncorrectQueryResults,

    #[error("response was not serialized correctly")]
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

    /// Call the RPC method [method] with argument [arg]; decode and return the output
    pub async fn call<A, R>(&mut self, method: impl AsRef<str>, arg: A) -> Result<R, JsonRpcError>
    where
        A: Serialize,
        R: DeserializeOwned,
    {
        let request = self.make_request(method, arg);

        self.send(&request).await?;
        let response: Response<R> = self.receive().await?;

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
        &mut self,
        method: impl AsRef<str>,
        args: impl IntoIterator<Item = A>,
    ) -> Result<impl Iterator<Item = R>, JsonRpcError> {
        let requests: Vec<Request<A>> = args
            .into_iter()
            .map(|arg| self.make_request(&method, arg))
            .collect();

        self.send(&requests).await?;
        let responses: BatchResponse<R> = self.receive().await?;

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

    /// Serialize `value` and write it to `self.output`
    async fn send<T: Serialize>(&mut self, value: &T) -> Result<(), JsonRpcError> {
        let mut request_json = serde_json::to_vec(&value).expect("requests should be serializable");
        request_json.push(b'\n');

        debug!(
            "sending request: {:?}",
            String::from_utf8_lossy(request_json.as_slice())
        );
        self.output.write_all(&request_json).await?;
        self.output.flush().await?;
        Ok(())
    }

    /// Read a line from `self.input` and deserialize it as a JSON T
    async fn receive<T: DeserializeOwned>(&mut self) -> Result<T, JsonRpcError> {
        let mut response_json = String::new();
        self.input.read_line(&mut response_json).await?;
        debug!("received:{response_json}");
        Ok(serde_json::de::from_str(response_json.as_str())?)
    }

    /// Generate a [Request] to call [method]([arg]) using [self.sqn]; [self.sqn]
    fn make_request<A: Serialize>(&mut self, method: impl AsRef<str>, arg: A) -> Request<A> {
        let request = Request::<A> {
            jsonrpc: TwoPointZero,
            method: method.as_ref().to_string(),
            params: arg,
            id: self.sqn,
        };
        self.sqn += 1;

        request
    }
}
