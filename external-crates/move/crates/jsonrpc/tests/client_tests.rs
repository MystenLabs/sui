use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf,
        SimplexStream, WriteHalf, simplex,
    },
    join,
};
use tracing::debug;
use tracing_subscriber::EnvFilter;

use jsonrpc::{
    client::{Endpoint, JsonRpcError},
    types::RemoteError,
};

type HarnessEndpoint = Endpoint<ReadHalf<SimplexStream>, WriteHalf<SimplexStream>>;

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

/// Set up an [Endpoint] that communicates over in-memory pipes; return it and the pipes
fn create_harness() -> (HarnessEndpoint, impl AsyncBufRead, impl AsyncWrite) {
    let _ = tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .try_init();

    let (endpoint_input, output) = simplex(4096);
    let (input, endpoint_output) = simplex(4096);
    (
        Endpoint::new(input, output),
        BufReader::new(endpoint_input),
        endpoint_output,
    )
}

/// Spawn a task to call `endpoint.call(method, data)`
async fn call(
    mut endpoint: HarnessEndpoint,
    method: &'static str,
    data: TestData1,
) -> Result<TestData2, JsonRpcError> {
    tokio::spawn(async move {
        debug!("calling");
        endpoint
            .call::<TestData1, TestData2>(method.to_string(), data)
            .await
    })
    .await
    .unwrap()
}

/// Spawn a task to execute `endpoint.batch_call(method, data)`
async fn batch_call(
    mut endpoint: HarnessEndpoint,
    method: &'static str,
    data: Vec<TestData1>,
) -> Result<Vec<TestData2>, JsonRpcError> {
    debug!("calling");
    endpoint
        .batch_call::<TestData1, TestData2>(method.to_string(), data)
        .await
        .map(|it| it.into_iter().collect())
}

/// Read a line from [output] and compare it to [expected]
async fn get_request(mut output: impl AsyncBufRead + Unpin + Send + 'static) -> serde_json::Value {
    debug!("reading");
    let mut line = String::new();
    output.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

/// Send [value] on [input]
async fn respond(mut input: impl AsyncWrite + Unpin + Send + 'static, value: serde_json::Value) {
    let mut output = value.to_string();
    output.push('\n');

    debug!("writing {output}");
    input.write_all(output.as_bytes()).await.unwrap();
}

/// Calling [Endpoint::call] has correct end-to-end behavior with a normal response
#[tokio::test]
async fn test_call_normal() {
    let (endpoint, output, input) = create_harness();

    let call = call(endpoint, "method_name", TestData1::new());
    let read = get_request(output);
    let write = respond(
        input,
        json!({ "jsonrpc": "2.0", "id": 0, "result": TestData2::new() }),
    );

    let (call, read, _) = join!(call, read, write);

    assert_eq!(
        read,
        json!({"jsonrpc": "2.0", "method": "method_name", "id": 0, "params": TestData1::new()}),
    );

    assert_eq!(call.unwrap(), TestData2::new());
}

/// Calling [Endpoint::call] has correct end-to-end behavior with an error response
#[tokio::test]
async fn test_call_error() {
    let (endpoint, _, input) = create_harness();

    let error = RemoteError {
        code: 4,
        message: "error message".to_string(),
        data: None,
    };

    let call = call(endpoint, "method_name", TestData1::new());
    let write = respond(input, json!({"jsonrpc":"2.0","id":0,"error":error}));

    let (call, _) = join!(call, write);

    let JsonRpcError::RemoteError(received_error) = call.unwrap_err() else {
        panic!("expected error")
    };

    assert_eq!(received_error, error);
}

/// [Endpoint::batch_call] has correct end-to-end behavior with only normal responses (in
/// unsorted order)
#[tokio::test]
async fn test_batch_call_normal() {
    let (endpoint, output, input) = create_harness();

    let data: Vec<TestData1> = vec![
        TestData1 {
            data1: "v0".to_string(),
        },
        TestData1 {
            data1: "v1".to_string(),
        },
    ];

    let call = batch_call(endpoint, "method_name", data);
    let read = get_request(output);

    let write = respond(
        input,
        json!([
            // Note: out of order; should be ok
            { "jsonrpc": "2.0", "id": 1, "result": { "data2": "r1" }},
            { "jsonrpc": "2.0", "id": 0, "result": { "data2": "r0" }},
        ]),
    );

    let (call, read, _) = join!(call, read, write);

    assert_eq!(
        read,
        json!([
            { "jsonrpc": "2.0", "method": "method_name", "id": 0, "params": { "data1": "v0" }},
            { "jsonrpc": "2.0", "method": "method_name", "id": 1, "params": { "data1": "v1" }},
        ])
    );

    let expected: Vec<TestData2> = vec![
        TestData2 {
            data2: "r0".to_string(),
        },
        TestData2 {
            data2: "r1".to_string(),
        },
    ];

    assert_eq!(call.unwrap(), expected);
}

/// [Endpoint::batch_call] has correct end-to-end behavior with a mix of normal and error
/// responses
#[tokio::test]
async fn test_batch_call_error() {
    let (endpoint, _, input) = create_harness();

    let call = batch_call(
        endpoint,
        "method_name",
        vec![TestData1::new(), TestData1::new()],
    );

    let error = RemoteError {
        code: 4,
        message: "error message".to_string(),
        data: None,
    };

    let write = respond(
        input,
        json!([
            { "jsonrpc": "2.0", "id": 0, "result": TestData2::new()},
            { "jsonrpc": "2.0", "id": 1, "error": error},
        ]),
    );

    let (call, _) = join!(call, write);

    let JsonRpcError::RemoteError(received_error) = call.unwrap_err() else {
        panic!("expected error")
    };

    assert_eq!(received_error, error);
}

/// [Endpoint::batch_call] fails gracefully with an incomplete batch response
#[tokio::test]
async fn test_batch_missing_results() {
    let (endpoint, _, input) = create_harness();

    let call = batch_call(
        endpoint,
        "method_name",
        vec![TestData1::new(), TestData1::new()],
    );

    let write = respond(
        input,
        json!([
            { "jsonrpc": "2.0", "id": 0, "result": TestData2::new()},
        ]),
    );

    let (call, _) = join!(call, write);

    let JsonRpcError::IncorrectQueryResults = call.unwrap_err() else {
        panic!("expected incorrect query result response")
    };
}

/// [Endpoint::call] fails gracefully with incorrectly serialized responses
#[tokio::test]
async fn test_call_bad_data() {
    let (endpoint, _, input) = create_harness();

    let call = call(endpoint, "method_name", TestData1::new());
    let write = respond(
        input,
        json!({"jsonrpc":"some garbage","id":0,"result":TestData2::new()}),
    );

    let (call, _) = join!(call, write);

    let JsonRpcError::SerializationError(_) = call.unwrap_err() else {
        panic!("expected deserialization failure")
    };
}

/// [Endpoint::batch_call] fails gracefully with incorrectly serialized responses
#[tokio::test]
async fn test_batch_bad_data() {
    let (endpoint, _, input) = create_harness();

    let call = batch_call(
        endpoint,
        "method_name",
        vec![TestData1::new(), TestData1::new()],
    );
    let write = respond(
        input,
        json!({"jsonrpc":"2.0","id":0,"result":TestData2::new()}),
    );

    let (call, _) = join!(call, write);

    let JsonRpcError::SerializationError(_) = call.unwrap_err() else {
        panic!("expected deserialization failure")
    };
}

/// [Endpoint::batch_call] fails gracefully with duplicate reponses
#[tokio::test]
async fn test_batch_duplicate_results() {
    let (endpoint, _, input) = create_harness();

    let call = batch_call(
        endpoint,
        "method_name",
        vec![TestData1::new(), TestData1::new()],
    );

    let write = respond(
        input,
        json!([
            { "jsonrpc": "2.0", "id": 1, "result": { "data2": "r1" }},
            { "jsonrpc": "2.0", "id": 1, "result": { "data2": "extra" }},
        ]),
    );

    let (call, _) = join!(call, write);

    let JsonRpcError::IncorrectQueryResults = call.unwrap_err() else {
        panic!("expected incorrect results error")
    };
}
