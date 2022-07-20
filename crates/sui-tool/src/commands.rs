// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::join_all;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::genesis::Genesis;

use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_types::{base_types::*, messages::*};

use clap::*;

#[derive(Parser)]
#[clap(
    name = "sui-tool",
    about = "Debugging utilities for sui",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum ToolCommand {
    /// Fetch the same object from all validators
    #[clap(name = "fetch-object")]
    FetchObject {
        #[clap(long, help = "The object ID to fetch")]
        id: ObjectID,

        #[clap(long, help = "Fetch object at a specific sequence")]
        version: Option<u64>,

        #[clap(
            long,
            help = "Validator to fetch from - if not specified, all validators are queried"
        )]
        validator: Option<AuthorityName>,

        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(long = "history", help = "show full history of object")]
        history: bool,

        /// Concise mode prints tabular output suitable for processing with unix tools. For
        /// instance, to quickly check that all validators agree on the history of an object:
        ///
        ///     $ sui-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///         --genesis $HOME/.sui/sui_config/genesis.blob \
        ///         --history --concise   --no-header \
        ///         | sort -k2 -k1 \
        ///         | uniq -f1 -c
        ///
        /// (Prints full history in concise mode, suppresses header, sorts by version and then
        /// validator name, uniqs and counts lines ignoring validator name.)
        #[clap(long = "concise", help = "show concise output")]
        concise: bool,

        #[clap(long = "no-header", help = "don't show header in concise output")]
        no_header: bool,
    },
}

fn make_clients(genesis: PathBuf) -> Result<BTreeMap<AuthorityName, NetworkAuthorityClient>> {
    let mut net_config = mysten_network::config::Config::new();
    net_config.connect_timeout = Some(Duration::from_secs(5));
    net_config.request_timeout = Some(Duration::from_secs(5));
    net_config.http2_keepalive_interval = Some(Duration::from_secs(5));

    let genesis = Genesis::load(genesis)?;

    let mut authority_clients = BTreeMap::new();

    for validator in genesis.validator_set() {
        let channel = net_config.connect_lazy(&validator.network_address)?;
        let client = NetworkAuthorityClient::new(channel);
        let public_key_bytes = validator.public_key();
        authority_clients.insert(public_key_bytes, client);
    }

    Ok(authority_clients)
}

type ObjectVersionResponses = Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>)>;
struct ObjectData {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, ObjectVersionResponses)>,
}

trait OptionDebug<T> {
    fn opt_debug(&self, def_str: &str) -> String;
}
trait OptionDisplay<T> {
    fn opt_display(&self, def_str: &str) -> String;
}

impl<T> OptionDebug<T> for Option<T>
where
    T: std::fmt::Debug,
{
    fn opt_debug(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{:?}", t),
        }
    }
}

impl<T> OptionDisplay<T> for Option<T>
where
    T: std::fmt::Display,
{
    fn opt_display(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{}", t),
        }
    }
}

struct ConciseObjectOutput(ObjectData);

impl ConciseObjectOutput {
    fn header() -> String {
        format!(
            "{:<66} {:<8} {:<66} {:<45}",
            "validator", "version", "digest", "parent_cert"
        )
    }
}

impl std::fmt::Display for ConciseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, versions) in &self.0.responses {
            for (version, resp) in versions {
                write!(
                    f,
                    "{:<66} {:<8}",
                    format!("{:?}", name),
                    version.map(|s| s.value()).opt_debug("-")
                )?;
                match resp {
                    Err(_) => writeln!(
                        f,
                        "{:<66} {:<45}",
                        "object-fetch-failed", "no-cert-available"
                    )?,
                    Ok(resp) => {
                        let objref = resp
                            .requested_object_reference
                            .map(|(_, _, digest)| digest)
                            .opt_debug("objref-not-available");
                        let cert = resp
                            .parent_certificate
                            .as_ref()
                            .map(|c| *c.digest())
                            .opt_debug("<genesis>");
                        write!(f, " {:<66} {:<45}", objref, cert)?;
                    }
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

struct VerboseObjectOutput(ObjectData);

impl std::fmt::Display for VerboseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object: {}", self.0.requested_id)?;

        for (name, versions) in &self.0.responses {
            writeln!(f, "validator: {:?}", name)?;

            for (version, resp) in versions {
                writeln!(
                    f,
                    "-- version: {}",
                    version.opt_debug("<version not available>")
                )?;

                match resp {
                    Err(e) => writeln!(f, "Error fetching object: {}", e)?,
                    Ok(resp) => {
                        let objref = resp.requested_object_reference.opt_debug("<no object>");
                        writeln!(f, "  -- ref: {}", objref)?;

                        write!(f, "  -- cert:")?;
                        match &resp.parent_certificate {
                            None => writeln!(f, " <genesis>")?,
                            Some(cert) => {
                                let cert = format!("{}", cert);
                                let cert = textwrap::indent(&cert, "     | ");
                                write!(f, "\n{}", cert)?;
                            }
                        }

                        if let Some(ObjectResponse { lock, .. }) = &resp.object_and_lock {
                            // TODO maybe show object contents if we can do so meaningfully.
                            writeln!(f, "  -- object: <data>")?;
                            writeln!(f, "  -- locked by: {}", lock.opt_debug("<not locked>"))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn get_object(
    client: &NetworkAuthorityClient,
    id: ObjectID,
    start_version: Option<u64>,
    full_history: bool,
) -> Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>)> {
    let mut ret = Vec::new();
    let mut version = start_version;

    loop {
        let resp = client
            .handle_object_info_request(ObjectInfoRequest {
                object_id: id,
                request_kind: match version {
                    None => ObjectInfoRequestKind::LatestObjectInfo(None),
                    Some(v) => ObjectInfoRequestKind::PastObjectInfo(SequenceNumber::from_u64(v)),
                },
            })
            .await
            .map_err(anyhow::Error::from);

        let resp_version = resp
            .as_ref()
            .ok()
            .and_then(|r| r.requested_object_reference)
            .map(|(_, v, _)| v.value());
        ret.push((resp_version.map(SequenceNumber::from), resp));

        version = match (version, resp_version) {
            (Some(v), _) | (None, Some(v)) => {
                if v == 0 || !full_history {
                    break;
                } else {
                    Some(v - 1)
                }
            }
            _ => break,
        };
    }

    ret
}

impl ToolCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::FetchObject {
                id,
                validator,
                genesis,
                version,
                history,
                concise,
                no_header,
            } => {
                let clients = make_clients(genesis)?;

                let responses = join_all(
                    clients
                        .iter()
                        .filter(|(name, _)| {
                            if let Some(v) = validator {
                                v == **name
                            } else {
                                true
                            }
                        })
                        .map(|(name, client)| async {
                            let object_versions = get_object(client, id, version, history).await;
                            (*name, object_versions)
                        }),
                )
                .await;

                let output = ObjectData {
                    requested_id: id,
                    responses,
                };

                if concise {
                    if !no_header {
                        println!("{}", ConciseObjectOutput::header());
                    }
                    print!("{}", ConciseObjectOutput(output));
                } else {
                    println!("{}", VerboseObjectOutput(output));
                }
            }
        }

        Ok(())
    }
}
