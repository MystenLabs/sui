#![allow(unused)]

//! Types and methods for communication between the package system and external resolvers.
//!
//! External dependencies are resolved in each environment as follows. First, all dependencies are
//! grouped by the resolver name (`<res>`). Then, each binary `<res>` is invoked with the command
//! line `<res> --resolve-deps`; a toml-formatted map of requests is passed to the binary on
//! standard input, and the results are read from standard output.
//!
//! An individual request contains a dependency and an optional environment ID (see [Request]). If
//! the environment ID is not present, the resolver should return the "default" resolution,
//! otherwise it should return the appropriate resolution for the given network.
//!
//! For example, when resolving the following manifest:
//! ```toml
//! [environments]
//! a = "chainID1"
//! b = "chainID2"
//!
//! [dependencies]
//! foo = { r.res1 = "@qux/foo", override = true }
//! bar = { r.res1 = [1,2,3] }
//! xxx = { r.res2 = "zzz" }
//! ```
//!
//! The binaries `res1` and `res2` will each be invoked with `--resolve-deps`
//! (in the case of dep-overrides, they may be invoked more than once). The input for `res1` would
//! be:
//! ```toml
//! flavor = "..."
//!
//! queries.q01 = { argument = "@qux/foo" }
//! queries.q02 = { argument = "@qux/foo", environment-id = "chainID1" }
//! queries.q03 = { argument = "@qux/foo", environment-id = "chainID2" }
//! queries.q04 = { argument = [1,2,3] }
//! queries.q05 = { argument = [1,2,3], environment-id = "chainID1" }
//! queries.q06 = { argument = [1,2,3], environment-id = "chainID2" }
//! ```
//!
//! The resolver will respond with a response for each query (see [Response]). The response is
//! either an array of errors or a successful resolution with a resolved dependency and an optional
//! array of warnings.
//!
//! ```toml
//! responses.q1 = { resolved = { git = "..." }, warnings = ["...", "..."] }
//! responses.q2 = { errors = ["...", "..."] }
//! ...
//! ```
//!
//! Anything the resolver prints on stderr will be logged using [tracing::info!]

use std::{collections::BTreeMap, io::BufRead, process::Stdio};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, Command},
};
use tracing::{debug, info};

/// A name of a binary to invoke for for resolution
pub type ResolverName = String;

/// An opaque identifier for queries and responses
pub type QueryID = String;

/// An identifier for the environment to perform resolution against
pub type EnvironmentID = String;

/// The argument that is passed to an external resolver to request dependency resolution
pub const RESOLVE_ARG: &str = "--resolve-deps";

/// The type of resolution requests that are passed to external resolvers
#[derive(Serialize, Deserialize)]
#[serde(bound = "", deny_unknown_fields)]
pub struct Request {
    /// An identifier for the move flavor that the resolution is being requested for
    pub flavor: String,

    #[serde(default)]
    pub queries: BTreeMap<QueryID, Query>,
}

/// The type of resolutions responses that external resolvers should return
#[derive(Serialize, Deserialize)]
#[serde(bound = "", deny_unknown_fields)]
pub struct Response {
    /// Responses from the resolver, keyed by id
    pub responses: BTreeMap<QueryID, QueryResult>,
}

/// An individual dependency that needs to be resolved
#[derive(Serialize, Deserialize, Debug)]
#[serde(bound = "", deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub struct Query {
    /// Which environment should be used for resolution; None indicates a request for the "default"
    /// resolution
    #[serde(default)]
    pub environment_id: Option<EnvironmentID>,

    /// The `<data>` part of `{ r.<res> = <data> }`
    pub argument: toml::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "", untagged, deny_unknown_fields)]
pub enum QueryResult {
    Error {
        errors: Vec<String>,
    },
    Success {
        #[serde(default)]
        warnings: Vec<String>,
        resolved: toml::Value,
    },
}

#[derive(Error, Debug)]
pub enum ResolutionError {
    #[error("I/O Error when running external resolver {resolver}")]
    IoError {
        resolver: ResolverName,

        #[source]
        source: std::io::Error,
    },

    /// This indicates that the resolver was faulty
    #[error("Resolver {resolver} returned an incorrectly-formatted response: {message}")]
    BadResolver {
        resolver: ResolverName,
        message: String,
    },

    /// This indicates that the resolver was functioning properly but couldn't resolve a dependency
    #[error(
        "Resolver failed to resolve dependency {query_id}. It reported the following errors: TODO"
    )]
    ResolverFailure {
        resolver: ResolverName,
        query_id: QueryID,
        query: Query,
        errors: Vec<String>,
    },
}

impl ResolutionError {
    fn io_error(resolver: &ResolverName, source: std::io::Error) -> Self {
        Self::IoError {
            resolver: resolver.clone(),
            source,
        }
    }

    fn bad_resolver(resolver: &ResolverName, source: String, message: impl AsRef<str>) -> Self {
        Self::BadResolver {
            resolver: resolver.clone(),
            message: message.as_ref().to_string(),
        }
    }
}

impl Request {
    pub fn new(flavor: String) -> Self {
        Self {
            flavor,
            queries: BTreeMap::new(),
        }
    }

    /// Run the external resolver [resolver] and return its response. The response is guaranteed to
    /// contain the same keys as [self]. This method also forwards the standard error from the
    /// resolver to this process's standard error
    pub async fn execute(&self, resolver: &ResolverName) -> anyhow::Result<Response> {
        let mut child = Command::new(resolver)
            .arg(RESOLVE_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ResolutionError::io_error(resolver, e))?;

        send_request(resolver, self, child.stdin.take().expect("stdin exists")).await;
        let response = recv_response(resolver, child).await?;

        for key in self.queries.keys() {
            if !response.responses.contains_key(key) {
                // TODO: failure here is a bad response from the resolver
                anyhow::bail!("External resolver didn't resolve {key}");
            }
        }

        for (key, result) in response.responses.iter() {
            if !self.queries.contains_key(key) {
                // TODO: failure here is a bad response from the resolver
                anyhow::bail!("External resolver returned extra result {key}");
            }

            let QueryResult::Success { .. } = result else {
                // TODO: failure here is an appropriate response from the resolver but indicates a
                // resolution failure
                anyhow::bail!("External resolver failed for {key}");
            };
        }

        Ok(response)
    }
}

impl Query {
    pub fn new(data: toml::Value, env: Option<EnvironmentID>) -> Self {
        Self {
            argument: data,
            environment_id: env,
        }
    }
}

/// Write [request] onto [stdin], performing appropriate logging using [resolver]
async fn send_request(resolver: &ResolverName, request: &Request, mut stdin: ChildStdin) {
    let request = toml::to_string(request).expect("Request serialization should not fail");

    debug!("Request to {resolver}");
    for line in request.lines() {
        debug!(resolver, "  │ {line}");
    }

    // we don't really care if there's a write error as long as we get an output
    tokio::spawn(async move { stdin.write_all(request.as_bytes()).await });
}

/// Read a response from [child]'s stdin; logging using [resolver]
async fn recv_response(resolver: &ResolverName, mut child: Child) -> anyhow::Result<Response> {
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| ResolutionError::io_error(resolver, e))?;

    debug!("{resolver} stdout");
    for line in output.stdout.lines() {
        debug!("  │ {}", line.unwrap());
    }

    if !output.stderr.is_empty() {
        info!("Output from {resolver}:");
        for line in output.stderr.lines() {
            info!("  │ {}", line.expect("reading from byte array can't fail"));
        }
    }

    // TODO: failure here indicates a bad response from the resolver
    let response = String::from_utf8(output.stdout)?;

    // TODO: check exit code of resolver?

    // TODO: failure here indicates a bad response from the resolver
    Ok(toml::from_str(&response)?)
}
