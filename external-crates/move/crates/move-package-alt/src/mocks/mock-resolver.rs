// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! A mock resolver that can return a few different response types depending on its argument.
//!
//! ```toml
//!
//! [dependencies._.r.mock-resolver]
//! # halt immediately with the following stdout/stderr/exit code:
//! stdout = "..."
//! stderr = "..."
//! exit_code = ...
//!
//!
//! [dependencies._.r.mock-resolver]
//! # respond with the given JSON RPC Result values, and print stderr
//! output.mainnet-id.result = { local = "." } # Dependency
//! output.default.error = { code = ... , message = "...", data = ... } # JSON RPC error
//! stderr = "..."
//! ```
//!

use std::{collections::BTreeMap, env, io::stdin};

use jsonrpc::types::{BatchRequest, JsonRpcResult, RequestID, Response, TwoPointZero};
use move_package_alt::schema::{EXTERNAL_RESOLVE_ARG, EXTERNAL_RESOLVE_METHOD};
use serde::Deserialize;
use tracing::debug;
use tracing_subscriber::EnvFilter;

type EnvironmentID = String;

#[derive(Deserialize)]
struct ResolveRequest {
    env: EnvironmentID,
    data: RequestData,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RequestData {
    /// Execution should be halted with [exit_code] and return the given [stdout]/[stderr]
    Stdio(Exit),

    /// [stderr] should be printed and [output] should be included in the output
    Echo(EchoRequest),
}

#[derive(Deserialize)]
struct Exit {
    stdout: String,

    #[serde(default)]
    stderr: Option<String>,

    #[serde(default)]
    exit_code: Option<i32>,
}

#[derive(Deserialize)]
struct EchoRequest {
    output: BTreeMap<EnvironmentID, JsonRpcResult<serde_json::Value>>,

    #[serde(default)]
    stderr: Option<String>,
}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    tracing_subscriber::fmt::fmt()
        .without_time()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    assert!(
        args.len() == 2 && args[1] == EXTERNAL_RESOLVE_ARG,
        "External resolver must be called with a single argument `{EXTERNAL_RESOLVE_ARG}`"
    );

    let responses: Vec<Response<serde_json::Value>> = parse_input()
        .into_iter()
        .map(|(id, request)| process_request(id, request))
        .collect();

    let output = serde_json::to_string(&responses).expect("response can be serialized");
    let debug_out = serde_json::to_string_pretty(&responses).expect("response can be serialized");
    debug!("Returning\n{debug_out}");
    println!("{output}");
}

/// Read a [Request] from [stdin]
fn parse_input() -> BTreeMap<RequestID, ResolveRequest> {
    let mut line = String::new();
    stdin().read_line(&mut line).expect("stdin can be read");

    debug!("resolver stdin:\n{line}");

    let batch: BatchRequest<ResolveRequest> = serde_json::from_str(&line)
        .expect("External resolver must be passed a JSON RPC batch request");

    batch
        .into_iter()
        .map(|req| {
            assert!(req.method == EXTERNAL_RESOLVE_METHOD);
            (req.id, req.params)
        })
        .collect()
}

/// Process [request], creating a [Response] with the given [id]
/// Ends the process if an [Exit] variant is discovered
fn process_request(id: RequestID, request: ResolveRequest) -> Response<serde_json::Value> {
    debug!("Resolving request `{id}` for environment {}.", request.env);
    match request.data {
        RequestData::Stdio(exit) => {
            if let Some(line) = exit.stderr {
                eprintln!("{line}");
            };
            print!("{}", exit.stdout);
            debug!("Stdout:\n{}", exit.stdout);

            std::process::exit(exit.exit_code.unwrap_or(0))
        }
        RequestData::Echo(process) => {
            if let Some(line) = process.stderr {
                eprintln!("{line}");
            };

            let env_key: String = request.env;
            let result = process
                .output
                .get(&env_key)
                .expect("output field contains all environment ids")
                .clone();
            Response {
                jsonrpc: TwoPointZero,
                id,
                result,
            }
        }
    }
}
