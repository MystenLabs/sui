// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use crate::consensus_utils::*;
use crate::{metrics::ConsensusMetrics, Consensus};
#[allow(unused_imports)]
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
#[cfg(test)]
use std::collections::{BTreeSet, VecDeque};
use test_utils::CommitteeFixture;
#[allow(unused_imports)]
use tokio::sync::mpsc::channel;
use tokio::sync::watch;
use types::ReconfigureNotification;

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn commit_one() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=4, &genesis, &keys);

    // Make one certificate with round 5 to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 5, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        store,
        cert_store,
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
        gc_depth,
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
        assert_eq!(output.certificate.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.certificate.round(), 2);
}

// Run for 8 dag rounds with one dead node node (that is not a leader). We should commit the leaders of
// rounds 2, 4, and 6.
#[tokio::test]
async fn dead_node() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make the certificates.
    let mut keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    keys.sort(); // Ensure we don't remove one of the leaders.
    let _ = keys.pop().unwrap();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (mut certificates, _) =
        test_utils::make_optimal_certificates(&committee, 1..=9, &genesis, &keys);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        store,
        cert_store,
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
        gc_depth,
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
        let expected = ((i - 1) / keys.len() as u64) + 1;
        assert_eq!(output.certificate.round(), expected);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.certificate.round(), 6);
}

// Run for 6 dag rounds. The leaders of round 2 does not have enough support, but the leader of
// round 4 does. The leader of rounds 2 and 4 should thus be committed upon entering round 6.
#[tokio::test]
async fn not_enough_support() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let mut keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    keys.sort();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Round 1: Fully connected graph.
    let nodes: Vec<_> = keys.iter().take(3).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 1..=1, &genesis, &nodes);
    certificates.extend(out);

    // Round 2: Fully connect graph. But remember the digest of the leader. Note that this
    // round is the only one with 4 certificates.
    let (leader_2_digest, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 2, parents.clone());
    certificates.push_back(certificate);

    let nodes: Vec<_> = keys.iter().skip(1).cloned().collect();
    let (out, mut parents) =
        test_utils::make_optimal_certificates(&committee, 2..=2, &parents, &nodes);
    certificates.extend(out);

    // Round 3: Only node 0 links to the leader of round 2.
    let mut next_parents = BTreeSet::new();

    let name = &keys[1];
    let (digest, certificate) =
        test_utils::mock_certificate(&committee, name.clone(), 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = &keys[2];
    let (digest, certificate) =
        test_utils::mock_certificate(&committee, name.clone(), 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = &keys[0];
    parents.insert(leader_2_digest);
    let (digest, certificate) =
        test_utils::mock_certificate(&committee, name.clone(), 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    parents = next_parents.clone();

    // Rounds 4, 5, and 6: Fully connected graph.
    let nodes: Vec<_> = keys.iter().take(3).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 4..=6, &parents, &nodes);
    certificates.extend(out);

    // Round 7: Send a single certificate to trigger the commits.
    let (_, certificate) = test_utils::mock_certificate(&committee, keys[0].clone(), 7, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        store,
        cert_store,
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
        gc_depth,
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
        assert_eq!(output.certificate.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.certificate.round(), 2);

    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.round(), 2);
    }
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.round(), 3);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.certificate.round(), 4);
}

// Run for 6 dag rounds. Node 0 (the leader of round 2) is missing for rounds 1 and 2,
// and reapers from round 3.
#[tokio::test]
async fn missing_leader() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let mut keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    keys.sort();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Remove the leader for rounds 1 and 2.
    let nodes: Vec<_> = keys.iter().skip(1).cloned().collect();
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &nodes);
    certificates.extend(out);

    // Add back the leader for rounds 3, 4, 5 and 6.
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 3..=6, &parents, &keys);
    certificates.extend(out);

    // Add a certificate of round 7 to commit the leader of round 4.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 7, parents.clone());
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee,
        store,
        cert_store,
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
        gc_depth,
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
        assert_eq!(output.certificate.round(), 1);
    }
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.round(), 2);
    }
    for _ in 1..=4 {
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.round(), 3);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.certificate.round(), 4);
}

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn epoch_change() {
    let fixture = CommitteeFixture::builder().build();
    let mut committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    let _consensus_handle = Consensus::spawn(
        committee.clone(),
        store,
        cert_store,
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        tusk,
        metrics,
        gc_depth,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Run for a few epochs.
    for epoch in 0..5 {
        // Make certificates for rounds 1 to 4.
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();

        let (mut certificates, next_parents) =
            test_utils::make_certificates_with_epoch(&committee, 1..=4, epoch, &genesis, &keys);

        // Make one certificate with round 5 to trigger the commits.
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            keys[0].clone(),
            5,
            epoch,
            next_parents,
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
            assert_eq!(output.certificate.epoch(), epoch);
            assert_eq!(output.certificate.round(), 1);
        }
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.epoch(), epoch);
        assert_eq!(output.certificate.round(), 2);

        // Move to the next epoch.
        committee.epoch = epoch + 1;
        let message = ReconfigureNotification::NewEpoch(committee.clone());
        tx_reconfigure.send(message).unwrap();
    }
}

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2. Then shutdown consensus and restart it in a
#[tokio::test]
async fn restart_with_new_committee() {
    let fixture = CommitteeFixture::builder().build();
    let mut committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();

    // Run for a few epochs.
    for epoch in 0..5 {
        // Spawn the consensus engine and sink the primary channel.
        let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
        let (tx_output, mut rx_output) = test_utils::test_channel!(1);
        let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

        let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
        let (tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);
        let store = make_consensus_store(&test_utils::temp_dir());
        let cert_store = make_certificate_store(&test_utils::temp_dir());
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let gc_depth = 50;
        let tusk = Tusk::new(committee.clone(), store.clone(), gc_depth);

        let handle = Consensus::spawn(
            committee.clone(),
            store,
            cert_store,
            rx_reconfigure,
            rx_waiter,
            tx_primary,
            tx_consensus_round_updates,
            tx_output,
            tusk,
            metrics.clone(),
            gc_depth,
        );
        tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

        // Make certificates for rounds 1 to 4.
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (mut certificates, next_parents) =
            test_utils::make_certificates_with_epoch(&committee, 1..=4, epoch, &genesis, &keys);

        // Make one certificate with round 5 to trigger the commits.
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            keys[0].clone(),
            5,
            epoch,
            next_parents,
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
            assert_eq!(output.certificate.epoch(), epoch);
            assert_eq!(output.certificate.round(), 1);
        }
        let output = sequence.next().unwrap();
        assert_eq!(output.certificate.epoch(), epoch);
        assert_eq!(output.certificate.round(), 2);

        // Move to the next epoch.
        committee.epoch = epoch + 1;
        let message = ReconfigureNotification::Shutdown;
        tx_reconfigure.send(message).unwrap();

        // Ensure consensus stopped.
        handle.await.unwrap();
    }
}
