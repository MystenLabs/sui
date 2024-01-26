// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use suioplib::cli::{
    incidents_cmd, pulumi_cmd, service_cmd, IncidentsArgs, PulumiArgs, ServiceArgs,
};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    FmtSubscriber,
};

#[derive(Parser, Debug)]
#[command(author="build@mystenlabs.com", version, about, long_about = None)]
pub(crate) struct SuiOpArgs {
    /// The resource type we're operating on.
    #[command(subcommand)]
    resource: Resource,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum Resource {
    #[clap(aliases = ["inc", "i"])]
    Incidents(IncidentsArgs),
    #[clap(aliases = ["p"])]
    Pulumi(PulumiArgs),
    #[clap(aliases = ["s", "svc"])]
    Service(ServiceArgs),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let args = SuiOpArgs::parse();
    match args.resource {
        Resource::Incidents(args) => {
            incidents_cmd(&args).await?;
        }
        Resource::Pulumi(args) => {
            pulumi_cmd(&args).await?;
        }
        Resource::Service(args) => {
            service_cmd(&args).await?;
        }
    }

    Ok(())
}
