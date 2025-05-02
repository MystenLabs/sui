// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! A mock resolver that simply returns the data passed to it as a dependency resolution
//!
//! If there are any inputs with an stdout field then the standard output will be whatever string
//! is input on the first one of them and the other inputs are ignored.
//!
//! If none of the inputs have a stdout field, then the result fields are all returned according to
//! the external resolver protocol.
//!

use std::{
    collections::BTreeMap,
    env,
    io::{read_to_string, stdin},
    process::ExitCode,
};

use serde::Deserialize;
use tracing::debug;
use tracing_subscriber::EnvFilter;

use move_package_alt::dependency::external_protocol::{
    QueryID, QueryResult, RESOLVE_ARG, Request, Response,
};

#[derive(Deserialize)]
#[serde(untagged)]
enum RequestData {
    /// Execution should be halted with [exit_code] and return the given [stdout]/[stderr]
    Stdio(Exit),

    /// [stderr] should be printed and [output] should be included in the output
    Echo(EchoValue),
}

#[derive(Deserialize)]
struct Exit {
    stdout: String,

    #[serde(default)]
    stderr: String,

    #[serde(default)]
    exit_code: Option<u8>,
}

#[derive(Deserialize)]
struct EchoValue {
    output: QueryResult,
    stderr: Option<String>,
}

pub fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    tracing_subscriber::fmt::fmt()
        .without_time()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    assert!(
        args.len() == 2 && args[1] == RESOLVE_ARG,
        "External resolver must be called with a single argument `{RESOLVE_ARG}`"
    );

    let responses: Result<BTreeMap<QueryID, EchoValue>, Exit> = parse_input()
        .queries
        .into_iter()
        .map(|(id, query)| {
            let env_str = match query.environment_id {
                Some(e) => format!("for environment {}", &e),
                None => "for default environment".to_string(),
            };

            debug!("Resolving request `{id}` {env_str}.");

            let data = RequestData::deserialize(query.argument)
                .expect("Argument to mock resolver is expected to be well-formed");

            match data {
                RequestData::Stdio(exit) => Err(exit),
                RequestData::Echo(process) => Ok((id, process)),
            }
        })
        .collect();

    generate_output(responses)
}

/// Read a [Request] from [stdin]
fn parse_input() -> Request {
    let stdin = read_to_string(stdin()).expect("Stdin can be read");
    debug!("resolver stdin:\n{stdin}");

    toml::from_str(&stdin)
        .expect("External resolver must be passed a TOML-formatted request on stdin")
}

/// Report [output] on [stdout], [stderr], and the process return value
fn generate_output(output: Result<BTreeMap<String, EchoValue>, Exit>) -> ExitCode {
    match output {
        Ok(responses) => {
            debug!("Producing stderr");
            let responses = responses
                .into_iter()
                .map(|(id, p)| {
                    if let Some(line) = p.stderr {
                        eprintln!("{line}");
                    };
                    (id, p.output)
                })
                .collect();

            debug!("Producing stdout");
            let response = Response { responses };
            println!(
                "{}",
                toml::to_string(&response).expect("response can be serialized")
            );

            debug!("Exiting (success)");
            ExitCode::SUCCESS
        }
        Err(exit) => {
            debug!("Producing stdout output");
            println!("{}", exit.stdout);
            debug!("Producing stderr output");
            eprintln!("{}", exit.stderr);
            debug!("Exiting");
            exit.exit_code.unwrap_or(0).into()
        }
    }
}
