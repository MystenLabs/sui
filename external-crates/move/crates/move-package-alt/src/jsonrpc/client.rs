use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

use super::types::{
    BatchRequest, BatchResponse, JsonRpcResult, RemoteError, Request, RequestID, Response,
    TwoPointZero,
};

/// An endpoint for RPC calls.
pub struct Endpoint<I: AsyncRead, O: AsyncWrite> {
    input: BufReader<I>,
    output: BufWriter<O>,
    sqn: RequestID,
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

        result.push(response.result.get::<R, JsonRpcError>()?);
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
