// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

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

use config::{
    ConsensusAPIGrpcParameters, Import, NetworkAdminServerParameters, Parameters,
    PrometheusMetricsParameters, Stake,
};
use crypto::PublicKey;
use insta::assert_json_snapshot;
use multiaddr::Multiaddr;
use narwhal_config as config;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Write,
};
use tempfile::tempdir;
use test_utils::CommitteeFixture;

#[test]
fn leader_election_rotates_through_all() {
    // this committee has equi-sized stakes
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let mut leader_counts = HashMap::new();
    // We most probably will only call `leader` on even rounds, so let's check this
    // still lets us use the whole roster of leaders.
    let mut leader_counts_stepping_by_2 = HashMap::new();
    for i in 0..100 {
        let leader = committee.leader(i);
        let leader_stepping_by_2 = committee.leader(i * 2);
        *leader_counts.entry(leader).or_insert(0) += 1;
        *leader_counts_stepping_by_2
            .entry(leader_stepping_by_2)
            .or_insert(0) += 1;
    }
    assert!(leader_counts.values().all(|v| *v >= 20));
    assert!(leader_counts_stepping_by_2.values().all(|v| *v >= 20));
}

#[test]
fn update_primary_network_info_test() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
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

    let fixture = CommitteeFixture::builder().build();
    let committee2 = fixture.committee();
    let invalid_new_info = committee2
        .authorities
        .iter()
        .map(|(pk, a)| (pk.clone(), (a.stake, a.primary_address.clone())))
        .collect::<BTreeMap<_, (Stake, Multiaddr)>>();
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

    let invalid_new_info = committee
        .authorities
        .iter()
        // change the stake
        .map(|(pk, a)| (pk.clone(), (a.stake + 1, a.primary_address.clone())))
        .collect::<BTreeMap<_, (Stake, Multiaddr)>>();
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

    let committee4 = committee.clone();
    let mut pk_n_stake = Vec::new();
    let mut addresses = Vec::new();

    committee4.authorities.iter().for_each(|(pk, a)| {
        pk_n_stake.push((pk.clone(), a.stake));
        addresses.push(a.primary_address.clone())
    });

    let mut rng = rand::thread_rng();
    addresses.shuffle(&mut rng);

    let new_info = pk_n_stake
        .into_iter()
        .zip(addresses)
        .map(|((pk, stk), addr)| (pk, (stk, addr)))
        .collect::<BTreeMap<PublicKey, (Stake, Multiaddr)>>();

    let mut comm = committee;
    let res = comm.update_primary_network_info(new_info.clone());
    assert!(res.is_ok());
    for (pk, a) in comm.authorities.iter() {
        assert_eq!(a.primary_address, new_info.get(pk).unwrap().1);
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
    let network_admin_server_parameters = NetworkAdminServerParameters {
        primary_network_admin_server_port: 1234,
        worker_network_admin_server_base_port: 5678,
    };

    let parameters = Parameters {
        consensus_api_grpc: consensus_api_grpc_parameters,
        prometheus_metrics: prometheus_metrics_parameters,
        network_admin_server: network_admin_server_parameters,
        ..Parameters::default()
    };
    assert_json_snapshot!("parameters", parameters)
}

#[test]
fn parameters_import_snapshot_matches() {
    // GIVEN
    let input = r#"{
         "header_num_of_batches_threshold": 32,
         "max_header_num_of_batches": 1000,
         "gc_depth": 50,
         "sync_retry_delay": "5s",
         "sync_retry_nodes": 3,
         "batch_size": 500000,
         "max_batch_delay": "100ms",
         "block_synchronizer": {
             "range_synchronize_timeout": "30s",
             "certificates_synchronize_timeout": "2s",
             "payload_synchronize_timeout": "3_000ms",
             "payload_availability_timeout": "4_000ms"
         },
         "consensus_api_grpc": {
             "socket_addr": "/ip4/127.0.0.1/tcp/0/http",
             "get_collections_timeout": "5_000ms",
             "remove_collections_timeout": "5_000ms"
         },
         "max_concurrent_requests": 500000,
         "prometheus_metrics": {
            "socket_addr": "/ip4/127.0.0.1/tcp/0/http"
         },
         "network_admin_server": {
           "primary_network_admin_server_port": 0,
           "worker_network_admin_server_base_port": 0
         },
         "anemo": {
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
    let rng = StdRng::from_seed([0; 32]);
    let fixture = CommitteeFixture::builder().rng(rng).build();
    let committee = fixture.committee();

    // we need authorities to be serialized in order
    let mut settings = insta::Settings::clone_current();
    settings.set_sort_maps(true);
    settings.bind(|| assert_json_snapshot!("committee", committee));
}

#[test]
fn workers_snapshot_matches() {
    // The shape of this configuration is load-bearing in the NW benchmarks,
    // and in Sui (prod)
    let rng = StdRng::from_seed([0; 32]);
    let fixture = CommitteeFixture::builder().rng(rng).build();
    let worker_cache = fixture.worker_cache();

    // we need authorities to be serialized in order
    let mut settings = insta::Settings::clone_current();
    settings.set_sort_maps(true);
    settings.bind(|| assert_json_snapshot!("worker_cache", worker_cache));
}
