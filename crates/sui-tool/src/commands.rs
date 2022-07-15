// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
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

struct ObjectOutput {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, ObjectInfoResponse)>,
}

impl std::fmt::Display for ObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object: {}", self.requested_id)?;

        for (name, resp) in &self.responses {
            writeln!(f, "-- validator: {:?}", name)?;

            let objref = resp
                .requested_object_reference
                .as_ref()
                .map(|o| format!("{:?}", o))
                .unwrap_or_else(|| "<no object>".into());
            writeln!(f, "---- ref: {}", objref)?;

            let cert = resp
                .parent_certificate
                .as_ref()
                .map(|c| format!("{:?}", c.digest()))
                .unwrap_or_else(|| "<genesis>".into());
            writeln!(f, "---- cert: {}", cert)?;

            if let Some(ObjectResponse { lock, .. }) = &resp.object_and_lock {
                // TODO maybe show object contents if we can do so meaningfully.
                writeln!(f, "---- object: <data>")?;
                writeln!(
                    f,
                    "---- locked by : {}",
                    lock.as_ref()
                        .map(|l| format!("{:?}", l.digest()))
                        .unwrap_or_else(|| "<not locked>".into())
                )?;
            }
        }
        Ok(())
    }
}

impl ToolCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::FetchObject {
                id,
                validator,
                genesis,
                version,
            } => {
                let clients = make_clients(genesis)?;

                let clients = clients.iter().filter(|(name, _)| {
                    if let Some(v) = validator {
                        v == **name
                    } else {
                        true
                    }
                });

                let mut output = ObjectOutput {
                    requested_id: id,
                    responses: Vec::new(),
                };

                for (name, client) in clients {
                    let resp = client
                        .handle_object_info_request(ObjectInfoRequest {
                            object_id: id,
                            request_kind: match version {
                                None => ObjectInfoRequestKind::LatestObjectInfo(None),
                                Some(v) => ObjectInfoRequestKind::PastObjectInfo(
                                    SequenceNumber::from_u64(v),
                                ),
                            },
                        })
                        .await?;

                    output.responses.push((*name, resp));
                }

                println!("{}", output);
            }
        }

        Ok(())
    }
}
