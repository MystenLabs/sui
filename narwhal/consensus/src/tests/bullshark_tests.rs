// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use crate::consensus_utils::*;
use crate::{metrics::ConsensusMetrics, Consensus, NUM_SHUTDOWN_RECEIVERS};
use fastcrypto::hash::Hash;
#[allow(unused_imports)]
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
#[cfg(test)]
use std::collections::{BTreeSet, VecDeque};
use test_utils::CommitteeFixture;
#[allow(unused_imports)]
use tokio::sync::mpsc::channel;
use tokio::sync::watch;
use tracing::info;
use types::PreSubscribedBroadcastSender;

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn commit_one() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make certificates for rounds 1 and 2.
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &keys);

    // Make two certificate (f+1) with round 3 to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 3, next_parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[1].clone(), 3, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        gc_depth,
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

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
        bullshark,
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
    let committed_sub_dag: CommittedSubDag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=4 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 2);

    // AND the reputation scores have not been updated
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 0);
}

// Run for 8 dag rounds with one dead node node (that is not a leader). We should commit the leaders of
// rounds 2, 4, and 6.
#[tokio::test]
async fn dead_node() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    // Make the certificates.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        gc_depth,
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

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
        bullshark,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus.
    tokio::spawn(async move {
        while let Some(certificate) = certificates.pop_front() {
            tx_waiter.send(certificate).await.unwrap();
        }
    });

    // We should commit 4 leaders (rounds 2, 4, 6, and 8).
    let mut committed = Vec::new();
    let mut committed_sub_dags: Vec<CommittedSubDag> = Vec::new();
    for _commit_rounds in 1..=4 {
        let committed_sub_dag = rx_output.recv().await.unwrap();
        committed.extend(committed_sub_dag.certificates.clone());
        committed_sub_dags.push(committed_sub_dag);
    }

    let mut sequence = committed.into_iter();
    for i in 1..=21 {
        let output = sequence.next().unwrap();
        let expected = ((i - 1) / keys.len() as u64) + 1;
        assert_eq!(output.round(), expected);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 8);

    // AND check that the consensus scores are the expected ones
    for (index, sub_dag) in committed_sub_dags.iter().enumerate() {
        // For the first commit we don't expect to have any score updates
        if index == 0 {
            assert_eq!(sub_dag.reputation_score.total_authorities(), 0);
        } else {
            // For any other commit we expect to always have a +1 score for each authority, as everyone
            // always votes for the leader
            for score in sub_dag.reputation_score.scores_per_authority.values() {
                assert_eq!(*score as usize, index);
            }
        }
    }
}

// Run for 5 dag rounds. The leader of round 2 does not have enough support, but the leader of
// round 4 does. The leader of rounds 2 and 4 should thus be committed (because they are linked).
#[tokio::test]
async fn not_enough_support() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;
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

    // Rounds 4: Fully connected graph. This is the where we "boost" the leader.
    let nodes: Vec<_> = keys.to_vec();
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 4..=4, &parents, &nodes);
    certificates.extend(out);

    // Round 5: Send f+1 certificates to trigger the commit of leader 4.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 5, parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, keys[1].clone(), 5, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        gc_depth,
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

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
        bullshark,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // We should commit 2 leaders (rounds 2 and 4).
    let committed_sub_dag: CommittedSubDag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for _ in 1..=3 {
        let output = sequence.next().unwrap();
        assert_eq!(output.round(), 1);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 2);

    // AND no scores exist for leader 2 , as this is the first commit
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 0);

    let committed_sub_dag: CommittedSubDag = rx_output.recv().await.unwrap();
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

    // AND scores should be updated with everyone that has voted for leader of round 2.
    // Only node 0 has voted for the leader of this round, so only 1 score should exist
    // with value 1
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 1);

    let node_0_name = &keys[0];
    let score = committed_sub_dag
        .reputation_score
        .scores_per_authority
        .get(node_0_name)
        .unwrap();
    assert_eq!(*score, 1);
}

// Run for 7 dag rounds. Node 0 (the leader of round 2) is missing for rounds 1 and 2,
// and reappears from round 3.
#[tokio::test]
async fn missing_leader() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;
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

    // Add back the leader for rounds 3 and 4.
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 3..=4, &parents, &keys);
    certificates.extend(out);

    // Add f+1 certificates of round 5 to commit the leader of round 4.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 5, parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, keys[1].clone(), 5, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        gc_depth,
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

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
        bullshark,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. We should only commit upon receiving the last
    // certificate, so calls below should not block the task.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the commit sequence is as expected.
    let committed_sub_dag: CommittedSubDag = rx_output.recv().await.unwrap();
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

    // AND no scores exist since this is the first commit that has happened
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 0);
}

// Run for 11 dag rounds in ideal conditions (all nodes reference all other nodes).
// Every two rounds (on odd rounds), restart consensus and check consistency.
#[tokio::test]
async fn committed_round_after_restart() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let epoch = committee.epoch();
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=11, epoch, &genesis, &keys);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());

    for input_round in (1..=11usize).step_by(2) {
        // Spawn consensus and create related channels.
        let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
        let (tx_output, mut rx_output) = test_utils::test_channel!(1);
        let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0);

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
        let gc_depth = 50;
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            gc_depth,
            metrics.clone(),
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

        let handle = Consensus::spawn(
            committee.clone(),
            gc_depth,
            store.clone(),
            cert_store.clone(),
            tx_shutdown.subscribe(),
            rx_waiter,
            tx_primary,
            tx_consensus_round_updates,
            tx_output,
            bullshark,
            metrics.clone(),
        );

        // When `input_round` is 2 * r + 1, r > 1, the previous commit round would be 2 * (r - 1),
        // and the expected commit round after sending in certificates up to `input_round` would
        // be 2 * r.

        let last_committed_round = rx_consensus_round_updates.borrow().to_owned() as usize;
        assert_eq!(last_committed_round, input_round.saturating_sub(3),);
        info!("Consensus started at last_committed_round={last_committed_round}");

        // Feed certificates from two rounds into consensus.
        let start_index = input_round.saturating_sub(2) * committee.size();
        let end_index = input_round * committee.size();
        for cert in certificates.iter().take(end_index).skip(start_index) {
            tx_waiter.send(cert.clone()).await.unwrap();
        }
        info!("Sent certificates {start_index} ~ {end_index} to consensus");

        // There should only be one new item in the output streams.
        if input_round > 1 {
            let committed = rx_output.recv().await.unwrap();
            info!(
                "Received output from consensus, committed_round={}",
                committed.leader.round()
            );
            let (round, _certs) = rx_primary.recv().await.unwrap();
            info!("Received committed certificates from consensus, committed_round={round}",);
        }

        // After sending inputs upt to round 2 * r + 1 to consensus, round 2 * r should have been
        // committed.
        assert_eq!(
            rx_consensus_round_updates.borrow().to_owned() as usize,
            input_round.saturating_sub(1),
        );
        info!(
            "Committed round adanced to {}",
            input_round.saturating_sub(1)
        );

        // Shutdown consensus and wait for it to stop.
        tx_shutdown.send().unwrap();
        handle.await.unwrap();
    }
}

/// Advance the DAG for 4 rounds, commit, and then send a certificate
/// from round 2. Certificate 2 should not get committed.
#[tokio::test]
async fn delayed_certificates_are_rejected() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=5, epoch, &genesis, &keys);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone());
    let mut bullshark = Bullshark::new(
        committee,
        store,
        gc_depth,
        metrics,
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    // Populate DAG with the rounds up to round 5 so we trigger commits
    let mut all_subdags = Vec::new();
    for certificate in certificates.clone() {
        let (_, committed_subdags) = bullshark
            .process_certificate(&mut state, certificate)
            .unwrap();
        all_subdags.extend(committed_subdags);
    }

    // ensure the leaders of rounds 2 and 4 have been committed
    assert_eq!(all_subdags.drain(0..).len(), 2);

    // now populate again the certificates of round 2 and 3
    // Since we committed everything of rounds <= 4, then those certificates should get rejected.
    for certificate in certificates.iter().filter(|c| c.round() <= 3) {
        let (outcome, _) = bullshark
            .process_certificate(&mut state, certificate.clone())
            .unwrap();

        assert_eq!(outcome, Outcome::CertificateBelowCommitRound);
    }
}

#[tokio::test]
async fn submitting_equivocating_certificate_should_error() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=1, epoch, &genesis, &keys);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone());
    let mut bullshark = Bullshark::new(
        committee.clone(),
        store,
        gc_depth,
        metrics,
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    // Populate DAG with all the certificates
    for certificate in certificates.clone() {
        let _ = bullshark
            .process_certificate(&mut state, certificate)
            .unwrap();
    }

    // Try to re-submit the exact same certificates - no error should be produced.
    for certificate in certificates {
        let _ = bullshark
            .process_certificate(&mut state, certificate)
            .unwrap();
    }

    // Try to submit certificates for same rounds but equivocating certificates (we just create
    // them with different epoch as a way to trigger the difference)
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=1, 100, &genesis, &keys);
    assert_eq!(certificates.len(), 4);

    for certificate in certificates {
        let err = bullshark
            .process_certificate(&mut state, certificate.clone())
            .unwrap_err();
        match err {
            ConsensusError::CertificateEquivocation(this_cert, _) => {
                assert_eq!(this_cert, certificate);
            }
            err => panic!("Unexpected error returned: {err}"),
        }
    }
}

/// Advance the DAG for 50 rounds, while we change "schedule" for every 5 subdag commits.
#[tokio::test]
async fn reset_consensus_scores_on_every_schedule_change() {
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 5;

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 50.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=50, epoch, &genesis, &keys);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone());
    let mut bullshark = Bullshark::new(
        committee,
        store,
        gc_depth,
        metrics,
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    // Populate DAG with the rounds up to round 50 so we trigger commits
    let mut all_subdags = Vec::new();
    for certificate in certificates {
        let (_, committed_subdags) = bullshark
            .process_certificate(&mut state, certificate)
            .unwrap();
        all_subdags.extend(committed_subdags);
    }

    // ensure the leaders of rounds 2 and 4 have been committed
    let mut current_score = 0;
    for sub_dag in all_subdags {
        // The first commit has no scores
        if sub_dag.sub_dag_index == 1 {
            assert_eq!(sub_dag.reputation_score.total_authorities(), 0);
        } else if sub_dag.sub_dag_index % NUM_SUB_DAGS_PER_SCHEDULE == 0 {
            // On every 5th commit we reset the scores and count from the beginning with
            // scores updated to 1, as we expect now every node to have voted for the previous leader.
            for score in sub_dag.reputation_score.scores_per_authority.values() {
                assert_eq!(*score as usize, 1);
            }
            current_score = 1;
        } else {
            // On every other commit the scores get calculated incrementally with +1 score
            // for every commit.
            current_score += 1;

            for score in sub_dag.reputation_score.scores_per_authority.values() {
                assert_eq!(*score, current_score);
            }

            if (sub_dag.sub_dag_index + 1) % NUM_SUB_DAGS_PER_SCHEDULE == 0 {
                // if this is going to be the last score update for the current schedule, then
                // make sure that the `fina_of_schedule` will be true
                assert!(sub_dag.reputation_score.final_of_schedule);
            } else {
                assert!(!sub_dag.reputation_score.final_of_schedule);
            }
        }
    }
}

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2. Then shutdown consensus and restart in a new epoch.
#[tokio::test]
async fn restart_with_new_committee() {
    let fixture = CommitteeFixture::builder().build();
    let mut committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

    // Run for a few epochs.
    for epoch in 0..5 {
        // Spawn the consensus engine and sink the primary channel.
        let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
        let (tx_output, mut rx_output) = test_utils::test_channel!(1);
        let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
        let store = make_consensus_store(&test_utils::temp_dir());
        let cert_store = make_certificate_store(&test_utils::temp_dir());
        let gc_depth = 50;
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            gc_depth,
            metrics.clone(),
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

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
            bullshark,
            metrics.clone(),
        );
        tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

        // Make certificates for rounds 1 and 2.
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (mut certificates, next_parents) =
            test_utils::make_certificates_with_epoch(&committee, 1..=2, epoch, &genesis, &keys);

        // Make two certificate (f+1) with round 3 to trigger the commits.
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            keys[0].clone(),
            3,
            epoch,
            next_parents.clone(),
        );
        certificates.push_back(certificate);
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            keys[1].clone(),
            3,
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
            assert_eq!(output.epoch(), epoch);
            assert_eq!(output.round(), 1);
        }
        let output = sequence.next().unwrap();
        assert_eq!(output.epoch(), epoch);
        assert_eq!(output.round(), 2);

        // Move to the next epoch.
        committee.epoch = epoch + 1;
        tx_shutdown.send().unwrap();

        // Ensure consensus stopped.
        handle.await.unwrap();
    }
}
