// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for the `#[path]`-included test modules.

use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::method;

/// Mock remote that reports every object lookup as "not found", through both the plain and the
/// checkpoint-scoped object queries. Execution routinely probes dynamic fields that exist
/// nowhere, and the fallible child-read path must see an authoritative remote miss rather than a
/// transport error, which propagates instead of reading as absent. Other query shapes still fail
/// fast (404), preserving the harness rule that tests pre-populate everything else they need.
pub(crate) async fn absent_objects_gql_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("multiGetObjects"))
        .and(body_string_contains("checkpoint(sequenceNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "checkpoint": { "query": { "multiGetObjects": [null] } } }
        })))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("multiGetObjects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "multiGetObjects": [null] }
        })))
        .mount(&server)
        .await;
    server
}
