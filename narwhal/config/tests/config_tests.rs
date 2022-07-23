// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use config::{
    Committee, ConsensusAPIGrpcParameters, Epoch, Parameters, PrimaryAddresses,
    PrometheusMetricsParameters, Stake,
};
use crypto::{ed25519::Ed25519PublicKey, traits::KeyPair};
use insta::assert_json_snapshot;
use rand::seq::SliceRandom;
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
            config::ComitteeUpdateError::MissingFromUpdate(_)
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
            config::ComitteeUpdateError::NotInCommittee(_)
                | config::ComitteeUpdateError::MissingFromUpdate(_)
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
            config::ComitteeUpdateError::DifferentStake(_)
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
        .collect::<BTreeMap<Ed25519PublicKey, (Stake, PrimaryAddresses)>>();

    let mut comm = committee;
    let res = comm.update_primary_network_info(new_info.clone());
    assert!(res.is_ok());
    for (pk, a) in comm.authorities.iter() {
        assert_eq!(a.primary, new_info.get(pk).unwrap().1);
    }
}

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
        socket_addr: "127.0.0.1:8081".parse().unwrap(),
    };

    let parameters = Parameters {
        consensus_api_grpc: consensus_api_grpc_parameters,
        prometheus_metrics: prometheus_metrics_parameters,
        ..Parameters::default()
    };
    assert_json_snapshot!("parameters", parameters)
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
