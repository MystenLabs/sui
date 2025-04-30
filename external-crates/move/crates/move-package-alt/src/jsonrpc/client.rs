use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tracing::debug;

use super::types::{
    BatchRequest, BatchResponse, JsonRpcResult, RemoteError, Request, RequestID, Response,
    TwoPointZero,
};

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

    #[error(transparent)]
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
            .ok_or_else(|| JsonRpcError::IncorrectQueryResults)?;

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

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use tokio::io::{
        simplex, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader,
        ReadHalf, SimplexStream, WriteHalf,
    };
    use tracing::debug;
    use tracing_subscriber::EnvFilter;

    use crate::jsonrpc::types::RemoteError;

    use super::{Endpoint, JsonRpcError};

    type HarnessEndpoint = Endpoint<ReadHalf<SimplexStream>, WriteHalf<SimplexStream>>;

    fn create_harness() -> (HarnessEndpoint, impl AsyncBufRead, impl AsyncWrite) {
        tracing_subscriber::fmt::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .without_time()
            .try_init();

        let (mut endpoint_input, mut output) = simplex(4096);
        let (mut input, mut endpoint_output) = simplex(4096);
        (
            Endpoint::new(input, output),
            BufReader::new(endpoint_input),
            endpoint_output,
        )
    }

    struct TestHarness {
        endpoint: Option<HarnessEndpoint>,
        endpoint_input: ReadHalf<SimplexStream>,
        endpoint_output: WriteHalf<SimplexStream>,
    }

    impl TestHarness {
        fn new() -> Self {
            let (mut endpoint_input, mut output) = simplex(4096);
            let (mut input, mut endpoint_output) = simplex(4096);
            Self {
                endpoint: Some(Endpoint::new(input, output)),
                endpoint_input,
                endpoint_output,
            }
        }

        /// Calls `self.endpoint.call(method, data)` in the background and returns the result;
        /// consumes self.endpoint
        async fn call(
            &mut self,
            method: impl AsRef<str>,
            data: TestData1,
        ) -> Result<TestData2, JsonRpcError> {
            todo!()
        }

        /// Calls `self.endpoint.batch_call(method, data)` and returns the result; consumes self.endpoint
        async fn batch_call(
            &mut self,
            method: impl AsRef<str>,
            data: Vec<TestData1>,
        ) -> Result<Vec<TestData2>, JsonRpcError> {
            todo!()
        }

        async fn receive(&mut self) -> std::io::Result<String> {
            todo!()
        }

        async fn send(&mut self, data: String) -> std::io::Result<()> {
            todo!()
        }
    }

    #[derive(Serialize, Deserialize, PartialEq, Eq)]
    struct TestData1 {
        data1: String,
    }

    impl TestData1 {
        fn new() -> Self {
            Self {
                data1: "value1".to_string(),
            }
        }
    }

    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
    struct TestData2 {
        data2: String,
    }

    impl TestData2 {
        fn new() -> Self {
            Self {
                data2: "value2".to_string(),
            }
        }
    }

    /// Read a line from [output] and decode it as a json value
    async fn expect_request(
        mut output: impl AsyncBufRead + Unpin + Send + 'static,
        expected: serde_json::Value,
    ) {
        tokio::spawn(async move {
            let mut line = String::new();
            output.read_line(&mut line).await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(json, expected);
        })
        .await
        .unwrap();
    }

    async fn respond(mut input: impl AsyncWrite + Unpin + Send, value: serde_json::Value) {
        let mut output = value.to_string();
        output.push('\n');

        input.write_all(output.as_bytes()).await.unwrap();
    }

    /// Calling [Endpoint::call] has correct end-to-end behavior with a normal response
    #[tokio::test]
    async fn test_call_normal() {
        let (mut endpoint, mut output, mut input) = create_harness();

        let call = endpoint.call::<TestData1, TestData2>("method_name", TestData1::new());
        expect_request(
            output,
            json!({
                "jsonrpc": "2.0",
                "method": "method_name",
                "id": 0,
                "params": TestData1::new()
            }),
        );

        respond(
            input,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "result": TestData2::new()
            }),
        );
        assert_eq!(call.await.unwrap(), TestData2::new());
    }

    /// Calling [Endpoint::call] has correct end-to-end behavior with an error response
    #[tokio::test]
    async fn test_call_error() {
        let (mut endpoint, mut output, mut input) = create_harness();

        let call = endpoint.call::<TestData1, TestData2>("method_name", TestData1::new());

        respond(
            input,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "error": { "code": 1, "message": "error message", "data": null },
            }),
        );

        let Err(JsonRpcError::RemoteError(error)) = call.await else {
            panic!()
        };
        assert_eq!(
            error,
            RemoteError {
                code: 1,
                message: "error message".to_string(),
                data: None
            }
        );
    }

    /// [Endpoint::batch_call] has correct end-to-end behavior with only normal responses (in
    /// unsorted order)
    #[tokio::test]
    async fn test_batch_call_normal() {
        let response = json!([
            {"jsonrpc":"2.0","id":0,"result":{"local":"for default"}},
            {"jsonrpc":"2.0","id":1,"result":{"local":"for mainnet"}}
        ]);
        todo!()
    }

    /// [Endpoint::batch_call] has correct end-to-end behavior with a mix of normal and error
    /// responses
    #[tokio::test]
    async fn test_batch_call_error() {
        todo!()
    }

    /// [Endpoint::batch_call] fails gracefully with an incomplete batch response
    #[tokio::test]
    async fn test_batch_missing_results() {
        todo!()
    }

    /// [Endpoint::call] fails gracefully with incorrectly serialized responses
    #[tokio::test]
    async fn test_call_bad_data() {
        todo!()
    }

    /// [Endpoint::batch_call] fails gracefully with incorrectly serialized responses
    #[tokio::test]
    async fn test_batch_bad_data() {
        todo!()
    }
}
