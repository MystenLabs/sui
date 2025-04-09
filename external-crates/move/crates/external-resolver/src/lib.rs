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
//! Anything the resolver prints on stderr will be passed through to this process's standard error

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

/// A name of a binary to invoke for for resolution
pub type ResolverName = String;

/// An opaque identifier for queries and responses
pub type QueryID = String;

/// An identifier for the environment to perform resolution against
pub type EnvironmentID = String;

/// The argument that is passed to an external resolver to request dependency resolution
pub const RESOLVE_ARG: &'static str = "--resolve-deps";

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
    /// The `<data>` part of `{ r.<res> = <data> }`
    pub argument: toml::Value,

    /// Which environment should be used for resolution; None indicates a request for the "default"
    /// resolution
    #[serde(default)]
    pub environment_id: Option<EnvironmentID>,
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

impl Request {
    pub fn new(flavor: String) -> Self {
        Self {
            flavor,
            queries: BTreeMap::new(),
        }
    }

    /// Run the external resolver [resolver] and return its response. The response is guaranteed to
    /// contain the same keys as [self]
    pub fn execute(&self, resolver: &ResolverName) -> anyhow::Result<Response> {
        let mut cmd = Command::new(resolver)
            .arg(RESOLVE_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let request = toml::to_string(self).unwrap();
        cmd.stdin.as_mut().unwrap().write_all(request.as_bytes())?;

        let mut response = String::new();
        cmd.stdout.as_mut().unwrap().read_to_string(&mut response)?;

        let result = toml::from_str::<Response>(&response)?;

        for key in self.queries.keys() {
            if !result.responses.contains_key(key) {
                anyhow::bail!("External resolver didn't resolve {key}");
            }
        }

        for (key, result) in result.responses.iter() {
            if !self.queries.contains_key(key) {
                anyhow::bail!("External resolver returned extra result {key}");
            }

            let QueryResult::Success { .. } = result else {
                anyhow::bail!("External resolver failed for {key}");
            };
        }

        Ok(result)
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
