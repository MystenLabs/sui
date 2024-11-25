// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rest_api::rest::info;
use sui_rest_api::rest::openapi;
use sui_rest_api::rest::ENDPOINTS;

#[test]
fn openapi_spec() {
    const OPENAPI_SPEC_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/openapi/openapi.json");

    let openapi = {
        let mut api = openapi::Api::new(info("unknown"));

        api.register_endpoints(ENDPOINTS.iter().copied());
        api.openapi()
    };

    let mut actual = serde_json::to_string_pretty(&openapi).unwrap();
    actual.push('\n');

    // Update the expected format
    if std::env::var_os("UPDATE").is_some() {
        std::fs::write(OPENAPI_SPEC_FILE, &actual).unwrap();
    }

    let expected = std::fs::read_to_string(OPENAPI_SPEC_FILE).unwrap();

    let diff = diffy::create_patch(&expected, &actual);

    if !diff.hunks().is_empty() {
        let formatter = if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
            diffy::PatchFormatter::new().with_color()
        } else {
            diffy::PatchFormatter::new()
        };
        let header = "Generated and checked-in openapi spec does not match. \
                          Re-run with `UPDATE=1` to update expected format";
        panic!("{header}\n\n{}", formatter.fmt_patch(&diff));
    }
}

#[tokio::test]
async fn openapi_explorer() {
    // Unless env var is set, just early return
    if std::env::var_os("OPENAPI_EXPLORER").is_none() {
        return;
    }

    let openapi = {
        let mut api = openapi::Api::new(info("unknown"));
        api.register_endpoints(ENDPOINTS.to_owned());
        api.openapi()
    };

    let router = openapi::OpenApiDocument::new(openapi).into_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    axum::serve(listener, router).await.unwrap();
}
