// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use anyhow::{Context, Result};
use clap::{crate_name, crate_version, App, AppSettings, ArgMatches, SubCommand};
use config::{Committee, Import, Parameters, WorkerId};
use consensus::{dag::Dag, Consensus, ConsensusOutput, ConsensusStore, SequenceNumber};
use crypto::{
    ed25519::{Ed25519KeyPair, Ed25519PublicKey},
    generate_production_keypair,
    traits::{KeyPair, VerifyingKey},
};
use primary::{
    BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, PayloadToken, Primary, Round,
};
use std::sync::Arc;
use store::{reopen, rocks, rocks::DBMap, Store};
use tokio::sync::mpsc::{channel, Receiver};
use tracing::{debug, subscriber::set_global_default};
use tracing_subscriber::filter::EnvFilter;
use worker::Worker;

/// The default channel capacity.
pub const CHANNEL_CAPACITY: usize = 1_000;

// The datastore column family names.
const HEADERS_CF: &str = "headers";
const CERTIFICATES_CF: &str = "certificates";
const PAYLOAD_CF: &str = "payload";
const BATCHES_CF: &str = "batches";
const LAST_COMMITTED_CF: &str = "last_committed";
const SEQUENCE_CF: &str = "sequence";

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A research implementation of Narwhal and Tusk.")
        .args_from_usage("-v... 'Sets the level of verbosity'")
        .subcommand(
            SubCommand::with_name("generate_keys")
                .about("Print a fresh key pair to file")
                .args_from_usage("--filename=<FILE> 'The file where to print the new key pair'"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Run a node")
                .args_from_usage("--keys=<FILE> 'The file containing the node keys'")
                .args_from_usage("--committee=<FILE> 'The file containing committee information'")
                .args_from_usage("--parameters=[FILE] 'The file containing the node parameters'")
                .args_from_usage("--store=<PATH> 'The path where to create the data store'")
                .subcommand(SubCommand::with_name("primary")
                    .about("Run a single primary")
                    .args_from_usage("-d, --consensus-disabled 'Provide this flag to run a primary node without Tusk'")
                )
                .subcommand(
                    SubCommand::with_name("worker")
                        .about("Run a single worker")
                        .args_from_usage("--id=<INT> 'The worker id'"),
                )
                .setting(AppSettings::SubcommandRequiredElseHelp),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    let tracing_level = match matches.occurrences_of("v") {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace",
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(tracing_level));

    cfg_if::cfg_if! {
        if #[cfg(feature = "benchmark")] {
            let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
            let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
                                     .with_env_filter(env_filter)
                                     .with_timer(timer).with_ansi(false);
        } else {
            let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
        }
    }
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();

    set_global_default(subscriber).expect("Failed to set subscriber");

    match matches.subcommand() {
        ("generate_keys", Some(sub_matches)) => {
            let kp = generate_production_keypair::<Ed25519KeyPair>();
            config::Export::export(&kp, sub_matches.value_of("filename").unwrap())
                .context("Failed to generate key pair")?
        }
        ("run", Some(sub_matches)) => run(sub_matches).await?,
        _ => unreachable!(),
    }
    Ok(())
}

// Runs either a worker or a primary.
async fn run(matches: &ArgMatches<'_>) -> Result<()> {
    let key_file = matches.value_of("keys").unwrap();
    let committee_file = matches.value_of("committee").unwrap();
    let parameters_file = matches.value_of("parameters");
    let store_path = matches.value_of("store").unwrap();

    // Read the committee and node's keypair from file.
    let keypair = Ed25519KeyPair::import(key_file).context("Failed to load the node's keypair")?;
    let committee =
        Committee::import(committee_file).context("Failed to load the committee information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters_file {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // Make the data store.
    let rocksdb = rocks::open_cf(
        store_path,
        None,
        &[
            HEADERS_CF,
            CERTIFICATES_CF,
            PAYLOAD_CF,
            BATCHES_CF,
            LAST_COMMITTED_CF,
            SEQUENCE_CF,
        ],
    )
    .expect("Failed creating database");

    let (header_map, certificate_map, payload_map, batch_map, last_committed_map, sequence_map) = reopen!(&rocksdb,
        HEADERS_CF;<HeaderDigest, Header<Ed25519PublicKey>>,
        CERTIFICATES_CF;<CertificateDigest, Certificate<Ed25519PublicKey>>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
        BATCHES_CF;<BatchDigest, Vec<u8>>,
        LAST_COMMITTED_CF;<Ed25519PublicKey, Round>,
        SEQUENCE_CF;<SequenceNumber, CertificateDigest>
    );

    let consensus_store = Arc::new(ConsensusStore::new(last_committed_map, sequence_map));
    let certificate_store = Store::new(certificate_map);

    // Channels the sequence of certificates.
    let (tx_output, rx_output) = channel(CHANNEL_CAPACITY);

    // Check whether to run a primary, a worker, or an entire authority.
    match matches.subcommand() {
        // Spawn the primary and consensus core.
        ("primary", Some(sub_matches)) => {
            let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
            let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);
            Primary::spawn(
                keypair.public().clone(),
                keypair,
                committee.clone(),
                parameters.clone(),
                Store::new(header_map),
                certificate_store.clone(),
                Store::new(payload_map),
                /* tx_consensus */ tx_new_certificates,
                /* rx_consensus */ rx_feedback,
            );

            if sub_matches.is_present("consensus-disabled") {
                debug!("Consensus is disabled. Will run without Tusk");
                Dag::spawn(rx_new_certificates);
            } else {
                Consensus::spawn(
                    committee,
                    consensus_store,
                    parameters.gc_depth,
                    /* rx_primary */ rx_new_certificates,
                    /* tx_primary */ tx_feedback,
                    tx_output,
                );
            }
        }

        // Spawn a single worker.
        ("worker", Some(sub_matches)) => {
            let id = sub_matches
                .value_of("id")
                .unwrap()
                .parse::<WorkerId>()
                .context("The worker id must be a positive integer")?;
            Worker::spawn(
                keypair.public().clone(),
                id,
                committee,
                parameters,
                Store::new(batch_map),
            );
        }
        _ => unreachable!(),
    }

    // Analyze the consensus' output.
    analyze(rx_output).await;

    // If this expression is reached, the program ends and all other tasks terminate.
    unreachable!();
}

/// Receives an ordered list of certificates and apply any application-specific logic.
async fn analyze<PublicKey: VerifyingKey>(mut rx_output: Receiver<ConsensusOutput<PublicKey>>) {
    while let Some(_message) = rx_output.recv().await {
        // NOTE: Here goes the application logic.
    }
}
