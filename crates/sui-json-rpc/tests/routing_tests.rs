// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use hyper::header::HeaderValue;
use hyper::HeaderMap;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::rpc_params;
use jsonrpsee::RpcModule;
use jsonrpsee_proc_macros::rpc;
use prometheus::Registry;
use std::env;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use sui_config::utils::get_available_port;
use sui_json_rpc::{JsonRpcServerBuilder, SuiRpcModule, CLIENT_TARGET_API_VERSION_HEADER};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;

#[tokio::test]
async fn test_rpc_backward_compatibility() {
    let mut builder = JsonRpcServerBuilder::new("1.5", &Registry::new());
    builder.register_module(TestApiModule).unwrap();

    let port = get_available_port("0.0.0.0");
    let _handle = builder
        .start(
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
            None,
        )
        .await
        .unwrap();
    let url = format!("http://0.0.0.0:{}", port);

    // Test with un-versioned client
    let client = HttpClientBuilder::default().build(&url).unwrap();
    let response: String = client.request("test_foo", rpc_params!(true)).await.unwrap();
    assert_eq!("Some string", response);

    // try to access old method directly should fail
    let client = HttpClientBuilder::default().build(&url).unwrap();
    let response: RpcResult<String> = client.request("test_foo_1_5", rpc_params!("string")).await;
    assert!(response.is_err());

    // Test with versioned client, version > backward compatible method version
    let mut versioned_header = HeaderMap::new();
    versioned_header.insert(
        CLIENT_TARGET_API_VERSION_HEADER,
        HeaderValue::from_static("1.6"),
    );
    let client_with_new_header = HttpClientBuilder::default()
        .set_headers(versioned_header)
        .build(&url)
        .unwrap();

    let response: String = client_with_new_header
        .request("test_foo", rpc_params!(true))
        .await
        .unwrap();
    assert_eq!("Some string", response);

    // Test with versioned client, version = backward compatible method version
    let mut versioned_header = HeaderMap::new();
    versioned_header.insert(
        CLIENT_TARGET_API_VERSION_HEADER,
        HeaderValue::from_static("1.5"),
    );
    let client_with_new_header = HttpClientBuilder::default()
        .set_headers(versioned_header)
        .build(&url)
        .unwrap();

    let response: String = client_with_new_header
        .request(
            "test_foo",
            rpc_params!("old version expect string as input"),
        )
        .await
        .unwrap();
    assert_eq!("Some string from old method", response);

    // Test with versioned client, version < backward compatible method version
    let mut versioned_header = HeaderMap::new();
    versioned_header.insert(
        CLIENT_TARGET_API_VERSION_HEADER,
        HeaderValue::from_static("1.4"),
    );
    let client_with_new_header = HttpClientBuilder::default()
        .set_headers(versioned_header)
        .build(&url)
        .unwrap();

    let response: String = client_with_new_header
        .request(
            "test_foo",
            rpc_params!("old version expect string as input"),
        )
        .await
        .unwrap();
    assert_eq!("Some string from old method", response);
}

#[tokio::test]
async fn test_disable_routing() {
    env::set_var("DISABLE_BACKWARD_COMPATIBILITY", "true");

    let mut builder = JsonRpcServerBuilder::new("1.5", &Registry::new());
    builder.register_module(TestApiModule).unwrap();

    let port = get_available_port("0.0.0.0");
    let _handle = builder
        .start(
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
            None,
        )
        .await
        .unwrap();
    let url = format!("http://0.0.0.0:{}", port);

    // try to access old method directly should fail
    let client = HttpClientBuilder::default().build(&url).unwrap();
    let response: RpcResult<String> = client.request("test_foo_1_5", rpc_params!("string")).await;
    assert!(response.is_err());

    // Test with versioned client, version = backward compatible method version, should fail because routing is disabled.
    let mut versioned_header = HeaderMap::new();
    versioned_header.insert(
        CLIENT_TARGET_API_VERSION_HEADER,
        HeaderValue::from_static("1.5"),
    );
    let client_with_new_header = HttpClientBuilder::default()
        .set_headers(versioned_header)
        .build(&url)
        .unwrap();

    let response: RpcResult<String> = client_with_new_header
        .request(
            "test_foo",
            rpc_params!("old version expect string as input"),
        )
        .await;
    assert!(response.is_err());
}

// TODO(chris): clean up this after March 27th, 2023
// #[tokio::test]
// async fn test_rpc_backward_compatibility_batched_request() {
//     let mut builder = JsonRpcServerBuilder::new("1.5", &Registry::new());
//     builder.register_module(TestApiModule).unwrap();

//     let port = get_available_port("0.0.0.0");
//     let handle = builder
//         .start(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)))
//         .await
//         .unwrap();
//     let url = format!("http://0.0.0.0:{}", port);

//     // Test with un-versioned client
//     let client = HttpClientBuilder::default().build(&url).unwrap();

//     let mut builder = BatchRequestBuilder::default();
//     builder.insert("test_foo", rpc_params!(true)).unwrap();
//     builder.insert("test_foo", rpc_params!(true)).unwrap();
//     builder.insert("test_foo", rpc_params!(true)).unwrap();

//     let response = client.batch_request::<String>(builder).await.unwrap();
//     assert_eq!(3, response.num_successful_calls());

//     // try to access old method directly should fail
//     let mut builder = BatchRequestBuilder::default();
//     builder.insert("test_foo_1_5", rpc_params!(true)).unwrap();
//     builder.insert("test_foo", rpc_params!(true)).unwrap();
//     builder.insert("test_foo", rpc_params!(true)).unwrap();

//     let response = client.batch_request::<String>(builder).await.unwrap();
//     assert_eq!(2, response.num_successful_calls());

//     // One malformed request shouldn't fail the whole batch
//     let client = Client::new();
//     let response = client
//         .post(format!("http://127.0.0.1:{}/", port))
//         .json(&vec![
//             json!(&Request {
//                 jsonrpc: Default::default(),
//                 id: Id::Number(1),
//                 method: "test_foo".into(),
//                 params: Some(&JsonRawValue::from_string("[true]".into()).unwrap()),
//             }),
//             json!("Bad json input"),
//         ])
//         .send()
//         .await
//         .unwrap();

//     let responses = response.text().await.unwrap();
//     let responses: Vec<&JsonRawValue> = serde_json::from_str(&responses).unwrap();

//     // Should have 2 results
//     assert_eq!(2, responses.len());

//     // First response should success
//     let response = serde_json::from_str::<Response<String>>(responses[0].get());
//     assert!(matches!(response, Ok(result) if result.result == "Some string"));

//     // Second response should fail
//     let response = serde_json::from_str::<ErrorResponse>(responses[1].get());
//     assert!(matches!(response, Ok(result) if result.error_object().message() == "Invalid request"));

//     handle.stop().unwrap()
// }

#[open_rpc(namespace = "test")]
#[rpc(server, client, namespace = "test")]
trait TestApi {
    #[method(name = "foo")]
    async fn foo(&self, some_bool: bool) -> RpcResult<String>;

    #[method(name = "foo", version <= "1.5")]
    async fn bar(&self, some_str: String) -> RpcResult<String>;
}

struct TestApiModule;

#[async_trait]
impl TestApiServer for TestApiModule {
    async fn foo(&self, _some_bool: bool) -> RpcResult<String> {
        Ok("Some string".into())
    }

    async fn bar(&self, _some_str: String) -> RpcResult<String> {
        Ok("Some string from old method".into())
    }
}

impl SuiRpcModule for TestApiModule {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }
    fn rpc_doc_module() -> Module {
        TestApiOpenRpc::module_doc()
    }
}
