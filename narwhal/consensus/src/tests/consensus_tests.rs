// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use fastcrypto::hash::Hash;
use std::collections::BTreeSet;
use storage::NodeStorage;
use telemetry_subscribers::TelemetryGuards;
use test_utils::{temp_dir, CommitteeFixture};
use tokio::sync::watch;

use crate::bullshark::Bullshark;
use crate::Consensus;
use crate::NUM_SHUTDOWN_RECEIVERS;
use types::{
    Certificate, CertificateAPI, HeaderAPI, PreSubscribedBroadcastSender, ReputationScores,
};

/// This test is trying to compare the output of the Consensus algorithm when:
/// (1) running without any crash for certificates processed from round 1 to 5 (inclusive)
/// (2) when a crash happens with last commit at round 2, and then consensus recovers
///
/// The output of (1) is compared to the output of (2) . The output of (2) is the combination
/// of the output before the crash and after the crash. What we expect to see is the output of
/// (1) & (2) be exactly the same. That will ensure:
/// * no certificates re-commit happens
/// * no certificates are skipped
/// * no forks created
#[tokio::test]
async fn test_consensus_recovery_with_bullshark() {
    let _guard = setup_tracing();

    // GIVEN
    let storage = NodeStorage::reopen(temp_dir(), None);

    let consensus_store = storage.consensus_store;
    let certificate_store = storage.certificate_store;

    // AND Setup consensus
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();

    // AND make certificates for rounds 1 to 7 (inclusive)
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=7, &genesis, &ids);

    // AND Spawn the consensus engine.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(100);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(100);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let gc_depth = 50;
    let bullshark = Bullshark::new(committee.clone(), consensus_store.clone(), gc_depth);

    let consensus_handle = Consensus::spawn(
        committee.clone(),
        gc_depth,
        consensus_store.clone(),
        certificate_store.clone(),
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
    );

    // WHEN we feed all certificates to the consensus.
    for certificate in certificates.iter() {
        // we store the certificates so we can enable the recovery
        // mechanism later.
        certificate_store.write(certificate.clone()).unwrap();
        tx_waiter.send(certificate.clone()).await.unwrap();
    }

    // THEN we expect to have 2 leader election rounds (round = 2, and round = 4).
    // In total we expect to have the following certificates get committed:
    // * 4 certificates from round 1
    // * 4 certificates from round 2
    // * 4 certificates from round 3
    // * 4 certificates from round 4
    // * 4 certificates from round 5
    // * 1 certificates from round 6 (the leader of last round)
    //
    // In total we should see 21 certificates committed
    let mut consensus_index_counter = 1;

    // hold all the certificates that get committed when consensus runs
    // without any crash.
    let mut committed_output_no_crash: Vec<Certificate> = Vec::new();
    let mut score_no_crash: ReputationScores = ReputationScores::default();

    'main: while let Some(sub_dag) = rx_output.recv().await {
        score_no_crash = sub_dag.reputation_score.clone();
        assert_eq!(sub_dag.sub_dag_index, consensus_index_counter);
        for output in sub_dag.certificates {
            assert!(output.round() <= 6);

            committed_output_no_crash.push(output.clone());

            // we received the leader of round 6, now stop as we don't expect to see any other
            // certificate from that or higher round.
            if output.round() == 6 {
                break 'main;
            }
        }
        consensus_index_counter += 1;
    }

    // AND the last committed store should be updated correctly
    let last_committed = consensus_store.read_last_committed();

    for id in ids.clone() {
        let last_round = *last_committed.get(&id).unwrap();

        // For the leader of round 6 we expect to have last committed round of 6.
        if id == Bullshark::leader_authority(&committee, 6) {
            assert_eq!(last_round, 6);
        } else {
            // For the others should be 5.
            assert_eq!(last_round, 5);
        }
    }

    // AND shutdown consensus
    consensus_handle.abort();

    // AND bring up consensus again. Store is clean. Now send again the same certificates
    // but up to round 3.
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(100);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(100);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let storage = NodeStorage::reopen(temp_dir(), None);

    let consensus_store = storage.consensus_store;
    let certificate_store = storage.certificate_store;

    let bullshark = Bullshark::new(committee.clone(), consensus_store.clone(), gc_depth);

    let consensus_handle = Consensus::spawn(
        committee.clone(),
        gc_depth,
        consensus_store.clone(),
        certificate_store.clone(),
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
    );

    // WHEN we send same certificates but up to round 3 (inclusive)
    // Then we store all the certificates up to round 6 so we can let the recovery algorithm
    // restore the consensus.
    // We omit round 7 so we can feed those later after "crash" to trigger a new leader
    // election round and commit.
    for certificate in certificates.iter() {
        if certificate.header().round() <= 3 {
            tx_waiter.send(certificate.clone()).await.unwrap();
        }
        if certificate.header().round() <= 6 {
            certificate_store.write(certificate.clone()).unwrap();
        }
    }

    // THEN we expect to commit with a leader of round 2.
    // So in total we expect to have committed certificates:
    // * 4 certificates of round 1
    // * 1 certificate of round 2 (the leader)
    let mut consensus_index_counter = 1;
    let mut committed_output_before_crash: Vec<Certificate> = Vec::new();

    'main: while let Some(sub_dag) = rx_output.recv().await {
        assert_eq!(sub_dag.sub_dag_index, consensus_index_counter);
        for output in sub_dag.certificates {
            assert!(output.round() <= 2);

            committed_output_before_crash.push(output.clone());

            // we received the leader of round 2, now stop as we don't expect to see any other
            // certificate from that or higher round.
            if output.round() == 2 {
                break 'main;
            }
        }
        consensus_index_counter += 1;
    }

    // AND shutdown (crash) consensus
    consensus_handle.abort();

    // AND bring up consensus again. Re-use the same store, so we can recover certificates
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(100);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(100);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let bullshark = Bullshark::new(committee.clone(), consensus_store.clone(), gc_depth);

    let _consensus_handle = Consensus::spawn(
        committee.clone(),
        gc_depth,
        consensus_store.clone(),
        certificate_store.clone(),
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
    );

    // WHEN send the certificates of round >= 5 to trigger a leader election for round 4
    // and start committing.
    for certificate in certificates.iter() {
        if certificate.header().round() >= 5 {
            tx_waiter.send(certificate.clone()).await.unwrap();
        }
    }

    // AND capture the committed output
    let mut committed_output_after_crash: Vec<Certificate> = Vec::new();
    let mut score_with_crash: ReputationScores = ReputationScores::default();

    'main: while let Some(sub_dag) = rx_output.recv().await {
        score_with_crash = sub_dag.reputation_score.clone();
        assert_eq!(score_with_crash.total_authorities(), 4);

        for output in sub_dag.certificates {
            assert!(output.round() >= 2);

            committed_output_after_crash.push(output.clone());

            // we received the leader of round 6, now stop as we don't expect to see any other
            // certificate from that or higher round.
            if output.round() == 6 {
                break 'main;
            }
        }
    }

    // THEN compare the output from a non-Crashed consensus to the outputs produced by the
    // crash consensus events. Those two should be exactly the same and will ensure that we see:
    // * no certificate re-commits
    // * no skips
    // * no forks
    committed_output_before_crash.append(&mut committed_output_after_crash);

    let all_output_with_crash = committed_output_before_crash;

    assert_eq!(committed_output_no_crash, all_output_with_crash);

    // AND ensure that scores are exactly the same
    assert_eq!(score_with_crash.scores_per_authority.len(), 4);
    assert_eq!(score_with_crash, score_no_crash);
    assert_eq!(
        score_with_crash
            .scores_per_authority
            .into_iter()
            .filter(|(_, score)| *score == 2)
            .count(),
        4
    );
}

fn setup_tracing() -> TelemetryGuards {
    // Setup tracing
    let tracing_level = "debug";
    let network_tracing_level = "info";

    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level}");

    telemetry_subscribers::TelemetryConfig::new()
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter)
        .init()
        .0
}
