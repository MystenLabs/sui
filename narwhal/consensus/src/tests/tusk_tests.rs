// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::mutable_key_type)]

use super::*;

use crate::consensus::ConsensusRound;
use crate::consensus_utils::*;
use crate::{metrics::ConsensusMetrics, Consensus, NUM_SHUTDOWN_RECEIVERS};
#[allow(unused_imports)]
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
#[cfg(test)]
use std::collections::{BTreeSet, VecDeque};
use test_utils::{latest_protocol_version, CommitteeFixture};
#[allow(unused_imports)]
use tokio::sync::mpsc::channel;
use tokio::sync::watch;
use types::PreSubscribedBroadcastSender;

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn commit_one() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make certificates for rounds 1 to 4.
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) = test_utils::make_optimal_certificates(
        &committee,
        1..=4,
        &genesis,
        &ids,
        &latest_protocol_version(),
    );

    // Make one certificate with round 5 to trigger the commits.
    let (_, certificate) = test_utils::mock_certificate(
        &committee,
        ids[0],
        5,
        next_parents,
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the first 4 ordered certificates are from round 1 (they are the parents of the committed
    // leader); then the leader's certificate should be committed.
    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=4 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 2);
}

// Run for 8 dag rounds with one dead node node (that is not a leader). We should commit the leaders of
// rounds 2, 4, and 6.
#[tokio::test]
async fn dead_node() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make the certificates.
    let mut ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    ids.sort(); // Ensure we don't remove one of the leaders.
    let _ = ids.pop().unwrap();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (mut certificates, _) = test_utils::make_optimal_certificates(
        &committee,
        1..=9,
        &genesis,
        &ids,
        &latest_protocol_version(),
    );

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus.
    tokio::spawn(async move {
        while let Some(certificate) = certificates.pop_front() {
            tx_waiter.send(certificate).await.unwrap();
        }
    });

    // We should commit 3 leaders (rounds 2, 4, and 6).
    let mut committed = Vec::new();
    let committed_sub_dag = rx_output.recv().await.unwrap();
    committed.extend(committed_sub_dag.certificates);
    let committed_sub_dag = rx_output.recv().await.unwrap();
    committed.extend(committed_sub_dag.certificates);
    let committed_sub_dag = rx_output.recv().await.unwrap();
    committed.extend(committed_sub_dag.certificates);

    let mut sequence = committed.into_iter();
    for i in 1..=15 {
        let output = sequence.next().unwrap();
        let expected = ((i - 1) / ids.len() as u64) + 1;
        assert_eq!(output.round(), expected);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 6);
}

// Run for 6 dag rounds. The leaders of round 2 does not have enough support, but the leader of
// round 4 does. The leader of rounds 2 and 4 should thus be committed upon entering round 6.
#[tokio::test]
async fn not_enough_support() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let mut ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    ids.sort();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Round 1: Fully connected graph.
    let nodes: Vec<_> = ids.iter().take(3).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(
        &committee,
        1..=1,
        &genesis,
        &nodes,
        &latest_protocol_version(),
    );
    certificates.extend(out);

    // Round 2: Fully connect graph. But remember the digest of the leader. Note that this
    // round is the only one with 4 certificates.
    let (leader_2_digest, certificate) = test_utils::mock_certificate(
        &committee,
        ids[0],
        2,
        parents.clone(),
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);

    let nodes: Vec<_> = ids.iter().skip(1).cloned().collect();
    let (out, mut parents) = test_utils::make_optimal_certificates(
        &committee,
        2..=2,
        &parents,
        &nodes,
        &latest_protocol_version(),
    );
    certificates.extend(out);

    // Round 3: Only node 0 links to the leader of round 2.
    let mut next_parents = BTreeSet::new();

    let name = ids[1];
    let (digest, certificate) = test_utils::mock_certificate(
        &committee,
        name,
        3,
        parents.clone(),
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = ids[2];
    let (digest, certificate) = test_utils::mock_certificate(
        &committee,
        name,
        3,
        parents.clone(),
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = ids[0];
    parents.insert(leader_2_digest);
    let (digest, certificate) = test_utils::mock_certificate(
        &committee,
        name,
        3,
        parents.clone(),
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);
    next_parents.insert(digest);

    parents = next_parents.clone();

    // Rounds 4, 5, and 6: Fully connected graph.
    let nodes: Vec<_> = ids.iter().take(3).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(
        &committee,
        4..=6,
        &parents,
        &nodes,
        &latest_protocol_version(),
    );
    certificates.extend(out);

    // Round 7: Send a single certificate to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, ids[0], 7, parents, &latest_protocol_version());
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // We should commit 2 leaders (rounds 2 and 4).
    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 2);

    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 2);
    }
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 3);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 4);
}

// Run for 6 dag rounds. Node 0 (the leader of round 2) is missing for rounds 1 and 2,
// and reapers from round 3.
#[tokio::test]
async fn missing_leader() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let mut ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    ids.sort();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Remove the leader for rounds 1 and 2.
    let nodes: Vec<_> = ids.iter().skip(1).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(
        &committee,
        1..=2,
        &genesis,
        &nodes,
        &latest_protocol_version(),
    );
    certificates.extend(out);

    // Add back the leader for rounds 3, 4, 5 and 6.
    let (out, parents) = test_utils::make_optimal_certificates(
        &committee,
        3..=6,
        &parents,
        &ids,
        &latest_protocol_version(),
    );
    certificates.extend(out);

    // Add a certificate of round 7 to commit the leader of round 4.
    let (_, certificate) = test_utils::mock_certificate(
        &committee,
        ids[0],
        7,
        parents.clone(),
        &latest_protocol_version(),
    );
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. We should only commit upon receiving the last
    // certificate, so calls below should not block the task.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the commit sequence is as expected.
    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 1);
    }
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 2);
    }
    for _ in 1..=4 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 3);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 4);
}

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2. Then shutdown consensus and restart it in a
#[tokio::test]
async fn restart_with_new_committee() {
    let fixture = CommitteeFixture::builder().build();
    let mut committee: Committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();

    // Run for a few epochs.
    for epoch in 0..5 {
        // Spawn the consensus engine and sink the primary channel.
        let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
        let (tx_output, mut rx_output) = test_utils::test_channel!(1);
        let (tx_consensus_round_updates, _rx_consensus_round_updates) =
            watch::channel(ConsensusRound::default());

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
        let store = make_consensus_store(&test_utils::temp_dir());
        let cert_store = make_certificate_store(&test_utils::temp_dir());
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let gc_depth = 50;
        let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);

        let handle = Consensus::spawn(
            committee.clone(),
            gc_depth,
            store,
            cert_store,
            tx_shutdown.subscribe(),
            rx_waiter,
            tx_primary,
            tx_consensus_round_updates,
            tx_output,
            tusk,
            metrics.clone(),
        );
        tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

        // Make certificates for rounds 1 to 4.
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (mut certificates, next_parents) = test_utils::make_certificates_with_epoch(
            &committee,
            1..=4,
            epoch,
            &genesis,
            &ids,
            &latest_protocol_version(),
        );

        // Make one certificate with round 5 to trigger the commits.
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            ids[0],
            5,
            epoch,
            next_parents,
            &latest_protocol_version(),
        );
        certificates.push_back(certificate);

        // Feed all certificates to the consensus. Only the last certificate should trigger
        // commits, so the task should not block.
        while let Some(certificate) = certificates.pop_front() {
            tx_waiter.send(certificate).await.unwrap();
        }

        // Ensure the first 4 ordered certificates are from round 1 (they are the parents of the committed
        // leader); then the leader's certificate should be committed.
        let committed_sub_dag = rx_output.recv().await.unwrap();
        let mut sequence = committed_sub_dag.certificates.into_iter();
        for _ in 1..=4 {
            let output = sequence.next().unwrap();
            assert_eq!(output.epoch(), epoch);
            assert_eq!(output.round(), 1);
        }
        let output = sequence.next().unwrap();
        assert_eq!(output.epoch(), epoch);
        assert_eq!(output.round(), 2);

        // Move to the next epoch.
        committee = committee.advance_epoch(epoch + 1);
        tx_shutdown.send().unwrap();

        // Ensure consensus stopped.
        handle.await.unwrap();
    }
}
