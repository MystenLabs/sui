// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests that detect changes in Narwhal configs and parameters.
// If a PR breaks one or more tests here, the PR probably has a real impact
// on a Narwhal configuration file. When test failure happens, the PR should
// be marked as a breaking change and reviewers should be aware of this.
//
// Owners and operators of production configuration files can add themselves to
// .github/CODEOWNERS for the corresponding snapshot tests, so they can get notified
// of changes. PRs that modifies snapshot files should wait for reviews from
// code owners (if any) before merging.
//
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta
// 1. Run `cargo insta test --review` under `./config`.
// 2. Review, accept or reject changes.

use std::collections::BTreeMap;

use config::{
    Committee, ConsensusAPIGrpcParameters, Epoch, Import, Parameters, PrimaryAddresses,
    PrometheusMetricsParameters, Stake,
};
use crypto::{traits::KeyPair as _, PublicKey};
use insta::assert_json_snapshot;
use rand::seq::SliceRandom;
use std::{fs::File, io::Write};
use tempfile::tempdir;
use test_utils::make_authority_with_port_getter;

#[test]
fn update_primary_network_info_test() {
    let committee = test_utils::committee(None);
    let res = committee
        .clone()
        .update_primary_network_info(BTreeMap::new())
        .unwrap_err();
    for err in res {
        assert!(matches!(
            err,
            config::CommitteeUpdateError::MissingFromUpdate(_)
        ))
    }

    let committee2 = test_utils::committee(42);
    let invalid_new_info = committee2
        .authorities
        .iter()
        .map(|(pk, a)| (pk.clone(), (a.stake, a.primary.clone())))
        .collect::<BTreeMap<_, (Stake, PrimaryAddresses)>>();
    let res2 = committee
        .clone()
        .update_primary_network_info(invalid_new_info)
        .unwrap_err();
    for err in res2 {
        // we'll get the two collections reporting missing from each other
        assert!(matches!(
            err,
            config::CommitteeUpdateError::NotInCommittee(_)
                | config::CommitteeUpdateError::MissingFromUpdate(_)
        ))
    }

    let committee3 = test_utils::committee(None);
    let invalid_new_info = committee3
        .authorities
        .iter()
        // change the stake
        .map(|(pk, a)| (pk.clone(), (a.stake + 1, a.primary.clone())))
        .collect::<BTreeMap<_, (Stake, PrimaryAddresses)>>();
    let res2 = committee
        .clone()
        .update_primary_network_info(invalid_new_info)
        .unwrap_err();
    for err in res2 {
        assert!(matches!(
            err,
            config::CommitteeUpdateError::DifferentStake(_)
        ))
    }

    let committee4 = test_utils::committee(None);
    let mut pk_n_stake = Vec::new();
    let mut addresses = Vec::new();

    committee4.authorities.iter().for_each(|(pk, a)| {
        pk_n_stake.push((pk.clone(), a.stake));
        addresses.push(a.primary.clone())
    });

    let mut rng = rand::thread_rng();
    addresses.shuffle(&mut rng);

    let new_info = pk_n_stake
        .into_iter()
        .zip(addresses)
        .map(|((pk, stk), addr)| (pk, (stk, addr)))
        .collect::<BTreeMap<PublicKey, (Stake, PrimaryAddresses)>>();

    let mut comm = committee;
    let res = comm.update_primary_network_info(new_info.clone());
    assert!(res.is_ok());
    for (pk, a) in comm.authorities.iter() {
        assert_eq!(a.primary, new_info.get(pk).unwrap().1);
    }
}

// If one or both of the parameters_xx_matches() tests are broken by a change, the following additional places are
// highly likely needed to be updated as well:
// 1. Docker/validators/parameters.json for starting Narwhal cluster with Docker Compose.
// 2. benchmark/fabfile.py for benchmarking a Narwhal cluster locally.
// 3. Sui configurations & snapshot tests when upgrading Narwhal in Sui to include the change.

#[test]
fn parameters_snapshot_matches() {
    // This configuration is load-bearing in the NW benchmarks,
    // and in Sui (prod config + shared object bench base). If this test breaks,
    // config needs to change in all of these.

    // We avoid defaults that randomly bind to a random port
    let consensus_api_grpc_parameters = ConsensusAPIGrpcParameters {
        socket_addr: "/ip4/127.0.0.1/tcp/8081/http".parse().unwrap(),
        ..ConsensusAPIGrpcParameters::default()
    };
    let prometheus_metrics_parameters = PrometheusMetricsParameters {
        socket_addr: "/ip4/127.0.0.1/tcp/8081/http".parse().unwrap(),
    };

    let parameters = Parameters {
        consensus_api_grpc: consensus_api_grpc_parameters,
        prometheus_metrics: prometheus_metrics_parameters,
        ..Parameters::default()
    };
    assert_json_snapshot!("parameters", parameters)
}

#[test]
fn parameters_import_snapshot_matches() {
    // GIVEN
    let input = r#"{
         "header_size": 1000,
         "max_header_delay": "100ms",
         "gc_depth": 50,
         "sync_retry_delay": "5s",
         "sync_retry_nodes": 3,
         "batch_size": 500000,
         "max_batch_delay": "100ms",
         "block_synchronizer": {
             "certificates_synchronize_timeout": "2s",
             "payload_synchronize_timeout": "3_000ms",
             "payload_availability_timeout": "4_000ms",
             "handler_certificate_deliver_timeout": "1_000ms"
         },
         "consensus_api_grpc": {
             "socket_addr": "/ip4/127.0.0.1/tcp/0/http",
             "get_collections_timeout": "5_000ms",
             "remove_collections_timeout": "5_000ms"
         },
         "max_concurrent_requests": 500000,
         "prometheus_metrics": {
            "socket_addr": "/ip4/127.0.0.1/tcp/0/http"
         }
      }"#;

    // AND temporary file
    let dir = tempdir().expect("Couldn't create tempdir");

    let file_path = dir.path().join("temp-properties.json");
    let mut file = File::create(file_path.clone()).expect("Couldn't create temp file");

    // AND write the json context
    writeln!(file, "{input}").expect("Couldn't write to file");

    // WHEN
    let params = Parameters::import(file_path.to_str().unwrap())
        .expect("Failed to import given Parameters json");

    // THEN
    assert_json_snapshot!("parameters_import", params)
}

#[test]
fn commmittee_snapshot_matches() {
    // The shape of this configuration is load-bearing in the NW benchmarks,
    // and in Sui (prod)
    let keys = test_utils::keys(None);

    let committee = Committee {
        epoch: Epoch::default(),
        authorities: keys
            .iter()
            .map(|kp| {
                let mut port = 0;
                let increment_port_getter = || {
                    port += 1;
                    port
                };
                (
                    kp.public().clone(),
                    make_authority_with_port_getter(increment_port_getter),
                )
            })
            .collect(),
    };
    // we need authorities to be serialized in order
    let mut settings = insta::Settings::clone_current();
    settings.set_sort_maps(true);
    settings.bind(|| assert_json_snapshot!("committee", committee));
}
