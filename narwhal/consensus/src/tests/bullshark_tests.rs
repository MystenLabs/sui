// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::mutable_key_type)]

use super::*;

use crate::consensus::ConsensusRound;
use crate::consensus_utils::NUM_SUB_DAGS_PER_SCHEDULE;
use crate::consensus_utils::*;
use crate::{metrics::ConsensusMetrics, Consensus, NUM_SHUTDOWN_RECEIVERS};
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
use types::{CertificateAPI, HeaderAPI, PreSubscribedBroadcastSender};

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn commit_one() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    // Make certificates for rounds 1 and 2.
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &ids);

    // Make two certificate (f+1) with round 3 to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, ids[0], 3, next_parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[1], 3, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_new_certificates,
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
        tx_new_certificates.send(certificate).await.unwrap();
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
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 4);
    assert!(committed_sub_dag.reputation_score.all_zero());
}

// Run for 11 dag rounds with one dead node node (that is not a leader). We should commit the leaders of
// rounds 2, 4, 6 and 10. The leader of round 8 will be missing, but eventually the leader 10 will get committed.
#[tokio::test]
async fn dead_node() {
    // Make the certificates.
    let fixture = CommitteeFixture::builder().build();
    let committee: Committee = fixture.committee();
    let mut ids: Vec<_> = committee
        .authorities()
        .map(|authority| authority.id())
        .collect();

    // remove the last authority - 4
    let dead_node = ids.pop().unwrap();

    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (mut certificates, _) =
        test_utils::make_optimal_certificates(&committee, 1..=11, &genesis, &ids);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_new_certificates,
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
            tx_new_certificates.send(certificate).await.unwrap();
        }
    });

    // We should commit 3 leaders (rounds 2, 4 and 6).
    let mut committed = Vec::new();
    let mut committed_sub_dags: Vec<CommittedSubDag> = Vec::new();
    for _commit_rounds in 1..=4 {
        let committed_sub_dag = rx_output.recv().await.unwrap();
        committed.extend(committed_sub_dag.certificates.clone());
        committed_sub_dags.push(committed_sub_dag);
    }

    let mut sequence = committed.into_iter();
    for i in 1..=27 {
        let output = sequence.next().unwrap();
        let expected = ((i - 1) / ids.len() as u64) + 1;
        assert_eq!(output.round(), expected);
    }
    let output = sequence.next().unwrap();
    assert_eq!(output.round(), 10);

    // AND check that the consensus scores are the expected ones
    for (index, sub_dag) in committed_sub_dags.iter().enumerate() {
        assert_eq!(sub_dag.reputation_score.total_authorities(), 4);

        // For the first commit we expect to have any only zero scores
        if index == 0 {
            sub_dag
                .reputation_score
                .scores_per_authority
                .iter()
                .for_each(|(_key, score)| {
                    assert_eq!(*score, 0_u64);
                });
        } else {
            // For any other commit we expect to always have a +1 score for each authority, as everyone
            // always votes for the leader
            for (key, score) in &sub_dag.reputation_score.scores_per_authority {
                if *key == dead_node {
                    assert_eq!(*score as usize, 0);
                } else {
                    assert_eq!(*score as usize, index);
                }
            }
        }
    }
}

// Run for 5 dag rounds. The leader of round 2 does not have enough support, but the leader of
// round 4 does. The leader of rounds 2 and 4 should thus be committed (because they are linked).
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
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 1..=1, &genesis, &nodes);
    certificates.extend(out);

    // Round 2: Fully connect graph. But remember the digest of the leader. Note that this
    // round is the only one with 4 certificates.
    let (leader_2_digest, certificate) =
        test_utils::mock_certificate(&committee, ids[0], 2, parents.clone());
    certificates.push_back(certificate);

    let nodes: Vec<_> = ids.iter().skip(1).cloned().collect();
    let (out, mut parents) =
        test_utils::make_optimal_certificates(&committee, 2..=2, &parents, &nodes);
    certificates.extend(out);

    // Round 3: Only node 0 links to the leader of round 2.
    let mut next_parents = BTreeSet::new();

    let name = ids[1];
    let (digest, certificate) = test_utils::mock_certificate(&committee, name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = ids[2];
    let (digest, certificate) = test_utils::mock_certificate(&committee, name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = ids[0];
    parents.insert(leader_2_digest);
    let (digest, certificate) = test_utils::mock_certificate(&committee, name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    parents = next_parents.clone();

    // Rounds 4: Fully connected graph. This is the where we "boost" the leader.
    let nodes: Vec<_> = ids.to_vec();
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 4..=4, &parents, &nodes);
    certificates.extend(out);

    // Round 5: Send f+1 certificates to trigger the commit of leader 4.
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[0], 5, parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[1], 5, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_new_certificates,
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
        tx_new_certificates.send(certificate).await.unwrap();
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

    // AND all scores are zero for leader 2 , as this is the first commit
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 4);
    assert!(committed_sub_dag.reputation_score.all_zero());

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
    // Only node 0 has voted for the leader of this round, so only their score should exist
    // with value 1, and everything else should be zero.
    assert_eq!(committed_sub_dag.reputation_score.total_authorities(), 4);

    let node_0_name: AuthorityIdentifier = ids[0];
    committed_sub_dag
        .reputation_score
        .scores_per_authority
        .iter()
        .for_each(|(key, score)| {
            if *key == node_0_name {
                assert_eq!(*score, 1_u64);
            } else {
                assert_eq!(*score, 0_u64);
            }
        });
}

// Run for 7 dag rounds. Node 0 (the leader of round 2) is missing for rounds 1 and 2,
// and reappears from round 3.
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
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &nodes);
    certificates.extend(out);

    // Add back the leader for rounds 3 and 4.
    let (out, parents) = test_utils::make_optimal_certificates(&committee, 3..=4, &parents, &ids);
    certificates.extend(out);

    // Add f+1 certificates of round 5 to commit the leader of round 4.
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[0], 5, parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[1], 5, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        store.clone(),
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_new_certificates,
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
        tx_new_certificates.send(certificate).await.unwrap();
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

    // AND all scores are zero since this is the first commit that has happened
    assert!(committed_sub_dag.reputation_score.all_zero());
}

// Run for 11 dag rounds in ideal conditions (all nodes reference all other nodes).
// Every two rounds (on odd rounds), restart consensus and check consistency.
#[tokio::test]
async fn committed_round_after_restart() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let epoch = committee.epoch();

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=11, epoch, &genesis, &ids);

    let store = make_consensus_store(&test_utils::temp_dir());
    let cert_store = make_certificate_store(&test_utils::temp_dir());

    for input_round in (1..=11usize).step_by(2) {
        // Spawn consensus and create related channels.
        let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(100);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(100);
        let (tx_output, mut rx_output) = test_utils::test_channel!(100);
        let (tx_consensus_round_updates, rx_consensus_round_updates) =
            watch::channel(ConsensusRound::new(0, 0));

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
        let gc_depth = 50;
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            metrics.clone(),
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

        let handle = Consensus::spawn(
            committee.clone(),
            gc_depth,
            store.clone(),
            cert_store.clone(),
            tx_shutdown.subscribe(),
            rx_new_certificates,
            tx_primary,
            tx_consensus_round_updates,
            tx_output,
            bullshark,
            metrics.clone(),
        );

        // When `input_round` is 2 * r + 1, r > 1, the previous commit round would be 2 * (r - 1),
        // and the expected commit round after sending in certificates up to `input_round` would
        // be 2 * r.

        let last_committed_round = rx_consensus_round_updates.borrow().committed_round as usize;
        assert_eq!(last_committed_round, input_round.saturating_sub(3),);
        info!("Consensus started at last_committed_round={last_committed_round}");

        // Feed certificates from two rounds into consensus.
        let start_index = input_round.saturating_sub(2) * committee.size();
        let end_index = input_round * committee.size();
        for cert in certificates.iter().take(end_index).skip(start_index) {
            cert_store.write(cert.clone()).unwrap();
            tx_new_certificates.send(cert.clone()).await.unwrap();
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

        // After sending inputs up to round 2 * r + 1 to consensus, round 2 * r should have been
        // committed.
        assert_eq!(
            rx_consensus_round_updates.borrow().committed_round as usize,
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
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=5, epoch, &genesis, &ids);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone(), gc_depth);
    let mut bullshark = Bullshark::new(committee, store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

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
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 11.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=1, epoch, &genesis, &ids);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone(), gc_depth);
    let mut bullshark =
        Bullshark::new(committee.clone(), store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

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
        test_utils::make_certificates_with_epoch(&committee, 1..=1, 100, &genesis, &ids);
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
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let epoch = committee.epoch();
    let gc_depth = 10;

    // Make certificates for rounds 1 to 50.
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (certificates, _) =
        test_utils::make_certificates_with_epoch(&committee, 1..=50, epoch, &genesis, &ids);

    let store = make_consensus_store(&test_utils::temp_dir());
    let mut state = ConsensusState::new(metrics.clone(), gc_depth);
    let mut bullshark = Bullshark::new(committee, store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

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
        // The first commit has all zero scores
        if sub_dag.sub_dag_index == 1 {
            assert!(sub_dag.reputation_score.all_zero());
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
    let mut committee: Committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();

    // Run for a few epochs.
    for epoch in 0..5 {
        // Spawn the consensus engine and sink the primary channel.
        let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
        let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
        let (tx_output, mut rx_output) = test_utils::test_channel!(1);
        let (tx_consensus_round_updates, _rx_consensus_round_updates) =
            watch::channel(ConsensusRound::new(0, 0));

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
        let store = make_consensus_store(&test_utils::temp_dir());
        let cert_store = make_certificate_store(&test_utils::temp_dir());
        let gc_depth = 50;
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            metrics.clone(),
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

        let handle = Consensus::spawn(
            committee.clone(),
            gc_depth,
            store,
            cert_store,
            tx_shutdown.subscribe(),
            rx_new_certificates,
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
            test_utils::make_certificates_with_epoch(&committee, 1..=2, epoch, &genesis, &ids);

        // Make two certificate (f+1) with round 3 to trigger the commits.
        let (_, certificate) = test_utils::mock_certificate_with_epoch(
            &committee,
            ids[0],
            3,
            epoch,
            next_parents.clone(),
        );
        certificates.push_back(certificate);
        let (_, certificate) =
            test_utils::mock_certificate_with_epoch(&committee, ids[1], 3, epoch, next_parents);
        certificates.push_back(certificate);

        // Feed all certificates to the consensus. Only the last certificate should trigger
        // commits, so the task should not block.
        while let Some(certificate) = certificates.pop_front() {
            tx_new_certificates.send(certificate).await.unwrap();
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

/// The test ensures the following things:
/// * garbage collection is removing the certificates from lower rounds according to gc depth only
/// * no certificate will ever get committed past the gc round
/// * existing uncommitted certificates in DAG (ex from slow nodes where no-one references them) they
/// get cleaned up.
#[tokio::test]
async fn garbage_collection_basic() {
    const GC_DEPTH: Round = 4;

    let fixture = CommitteeFixture::builder().build();
    let committee: Committee = fixture.committee();

    // We create certificates for rounds 1 to 7. For the authorities 1 to 3 the references
    // to previous rounds between them are fully connected - meaning that we always add all the parents
    // from previous round, except for the authority 4 which we consider it to be slow, so no one
    // refers to their certificates. Authority 4 still produces certificates, and uses as parents
    // all the certificates of the other authorities but never anyone else is referring to its
    // certificates. That will create a lone chain for authority 4. We should not see any certificate
    // committed for authority 4.
    let ids: Vec<AuthorityIdentifier> = committee
        .authorities()
        .map(|authority| authority.id())
        .collect();
    let slow_node = ids[3];
    let genesis = Certificate::genesis(&committee);

    let slow_nodes = vec![(slow_node, 0.0_f64)];
    let (certificates, _round_5_certificates) = test_utils::make_certificates_with_slow_nodes(
        &committee,
        1..=7,
        genesis,
        &ids,
        slow_nodes.as_slice(),
    );

    // Create Bullshark consensus engine
    let store = make_consensus_store(&test_utils::temp_dir());

    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let mut state = ConsensusState::new(metrics.clone(), GC_DEPTH);
    let mut bullshark = Bullshark::new(committee, store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

    // Now start feeding the certificates per round
    for c in certificates {
        let (_, sub_dags) = bullshark.process_certificate(&mut state, c).unwrap();

        sub_dags.iter().for_each(|sub_dag| {
            // ensure nothing has been committed for authority 4
            assert!(
                !sub_dag
                    .certificates
                    .iter()
                    .any(|c| c.header().author() == slow_node),
                "Slow authority shouldn't be amongst the committed ones"
            );

            // Once leader of round 6 is committed then we know that garbage
            // collection has run. In this case no certificate of round 1 should exist.
            if sub_dag.leader.round() == 6 {
                assert_eq!(
                    state
                        .dag
                        .iter()
                        .filter(|(round, _)| **round <= 2_u64)
                        .count(),
                    0,
                    "Didn't expect to still have certificates from round 1 and 2"
                );
            }

            // When we do commit for authorities, we always keep the certificates up to their latest
            // commit round + 1. Since we always commit for authorities 1 to 3 we expect to see no
            // certificates for them, but only for the slow authority 4 for which we never commit.
            // In this case the highest commit round for the authorities should be the leader.round() - 1,
            // except for the latest leader which should be leader.round().
            for (_round, certificates) in state
                .dag
                .iter()
                .filter(|(round, _)| **round <= sub_dag.leader.round())
            {
                assert_eq!(
                    certificates.len(),
                    4,
                    "We expect to have all the certificates"
                );
            }
        })
    }
}

// This test ensures that:
// * a slow node will never commit anything until its certificates get linked by others
// * certificates arriving bellow the gc round will never get committed
#[tokio::test]
async fn slow_node() {
    const GC_DEPTH: Round = 4;

    let fixture = CommitteeFixture::builder().build();
    let committee: Committee = fixture.committee();

    // We create certificates for rounds 1 to 8. For the authorities 1 to 3 the references
    // to previous rounds between them are fully connected - meaning that we always add all the parents
    // from previous round, except for the authority 4 which we consider it to be slow, so no one
    // refers to their certificates. Authority 4 still produces certificates, and uses as parents
    // all the certificates of the other authorities but never anyone else is referring to its
    // certificates. That will create a lone chain for authority 4. We should not see any certificate
    // committed for authority 4.
    let ids: Vec<AuthorityIdentifier> = committee
        .authorities()
        .map(|authority| authority.id())
        .collect();
    let slow_node = ids[3];
    let genesis = Certificate::genesis(&committee);

    let slow_nodes = vec![(slow_node, 0.0_f64)];
    let (certificates, round_8_certificates) = test_utils::make_certificates_with_slow_nodes(
        &committee,
        1..=8,
        genesis,
        &ids,
        slow_nodes.as_slice(),
    );

    let mut certificates: VecDeque<Certificate> = certificates;
    let mut slow_node_certificates = VecDeque::new();

    // Now we keep only the certificates from authorities 1-3
    certificates.retain(|c| {
        if c.origin() == slow_node {
            // if it is slow node's add it to the dedicated vec
            slow_node_certificates.push_back(c.clone());
            return false;
        }
        true
    });

    // Create Bullshark consensus engine
    let store = make_consensus_store(&test_utils::temp_dir());
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let mut state = ConsensusState::new(metrics.clone(), GC_DEPTH);
    let mut bullshark =
        Bullshark::new(committee.clone(), store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

    // Now start feeding the certificates per round up to 8. We expect to have
    // triggered a commit up to round 6 and gc round 1 & 2.
    for c in certificates {
        let _ = bullshark
            .process_certificate(&mut state, c.clone())
            .unwrap();
    }

    // We expect everything to have been cleaned up by standard gc until round 2 (included)
    assert_eq!(
        state
            .dag
            .iter()
            .filter(|(round, _)| **round <= 2_u64)
            .count(),
        0,
        "Didn't expect to still have certificates from round 1 and 2"
    );

    // Now send the certificates of slow node. Now leader election can't happen for round 8
    // as we haven't sent yet certificates of round 9
    for c in slow_node_certificates {
        let _ = bullshark.process_certificate(&mut state, c).unwrap();
    }

    // Now create the certificates of round 9, and ensure everyone gives support to
    // leader of round 8 (the slow node - 4) so we can trigger a commit. Also since slow node
    // refers to always all the parents of previous rounds, there will be a link to the previous
    // leader, so commit should be triggered immediately.
    // It is reminded that the leader election for testing is round robin, thus we can deterministically
    // know the leader of each round.
    let (certificates, _) = test_utils::make_certificates_with_slow_nodes(
        &committee,
        9..=9,
        round_8_certificates,
        &ids,
        &[],
    );

    // send the certificates - they should trigger a commit.
    // We should see certificates for authorities 1-3 only for round 7
    // We should see certificates for authority 4 only for round > 1 , as round 1 should have been
    // garbage collected.
    let mut committed = false;
    for c in certificates {
        let (outcome, sub_dags) = bullshark.process_certificate(&mut state, c).unwrap();

        match outcome {
            Outcome::NotEnoughSupportForLeader => {}
            Outcome::LeaderBelowCommitRound => {}
            Outcome::Commit => {
                assert_eq!(sub_dags.len(), 1);

                let sub_dag = sub_dags.get(0).unwrap();

                for committed in &sub_dag.certificates {
                    assert!(
                        committed.round() >= 2,
                        "We don't expect to see any certificate below round 2 because of gc"
                    );
                }

                let slow_node_total = sub_dag
                    .certificates
                    .iter()
                    .filter(|c| c.origin() == slow_node)
                    .count();

                assert_eq!(slow_node_total, 4);

                committed = true;
            }
            _ => panic!("Unexpected outcome {:?}", outcome),
        }
    }

    assert!(committed, "We expect to have commit for round 8");
}

/// This test creates a DAG that:
/// * contains a leader has not enough support at round 2
/// * a leader is missing at round 4
/// * gc happens on commit of leader round 6
/// * a dead node (node 4) for the first 3 rounds
/// * a dead node (node 1) for the last 3 rounds
#[tokio::test]
async fn not_enough_support_and_missing_leaders_and_gc() {
    const GC_DEPTH: Round = 4;

    let fixture = CommitteeFixture::builder().build();
    let committee: Committee = fixture.committee();

    let ids: Vec<AuthorityIdentifier> = committee
        .authorities()
        .map(|authority| authority.id())
        .collect();

    // take the first 3 nodes only - 4th one won't propose anything
    let keys_with_dead_node = ids[0..=2].to_vec();
    let slow_node = ids[3];
    let slow_nodes = vec![(slow_node, 0.0_f64)];
    let genesis = Certificate::genesis(&committee);

    let (mut certificates, round_2_certificates) = test_utils::make_certificates_with_slow_nodes(
        &committee,
        1..=2,
        genesis,
        &keys_with_dead_node,
        &slow_nodes,
    );

    // on round 3 we'll create certificates that don't provide f+1 support to round 2.
    let mut round_3_certificates: Vec<Certificate> = Vec::new();
    let first_node = keys_with_dead_node.get(0).unwrap();
    for id in &keys_with_dead_node {
        // Only the first one will provide support to it's own certificate apart from the others
        if id == first_node {
            let parents = round_2_certificates
                .iter()
                .map(|cert| cert.digest())
                .collect::<BTreeSet<_>>();
            let (_, certificate) = test_utils::mock_certificate(&committee, *id, 3, parents);
            round_3_certificates.push(certificate);
        } else {
            // we filter out the round 2 leader
            let parents = round_2_certificates
                .iter()
                .filter(|cert| cert.origin() != *first_node)
                .map(|cert| cert.digest())
                .collect::<BTreeSet<_>>();
            let (_, certificate) = test_utils::mock_certificate(&committee, *id, 3, parents);
            round_3_certificates.push(certificate);
        }
    }

    // on round 4 we create a missing leader (node 2)
    let mut round_4_certificates = Vec::new();
    let missing_leader = &ids[1];
    for id in ids.iter().filter(|a| *a != missing_leader) {
        let parents = round_3_certificates
            .iter()
            .map(|cert| cert.digest())
            .collect::<BTreeSet<_>>();
        let (_, certificate) = test_utils::mock_certificate(&committee, *id, 4, parents);
        round_4_certificates.push(certificate);
    }

    // now from round 5 to 7 create all certificates. Node 1 is now a slow node and won't create
    // referrencies to the certificates of that one.
    let slow_node = ids[0];
    let slow_nodes = vec![(slow_node, 0.0_f64)];

    let (certificates_5_to_7, _round_7_certificates) =
        test_utils::make_certificates_with_slow_nodes(
            &committee,
            5..=7,
            round_4_certificates.clone(),
            &ids,
            &slow_nodes,
        );

    // now send all certificates to Bullshark
    certificates.extend(round_3_certificates);
    certificates.extend(round_4_certificates);
    certificates.extend(certificates_5_to_7);

    // Create Bullshark consensus engine
    let store = make_consensus_store(&test_utils::temp_dir());
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let mut state = ConsensusState::new(metrics.clone(), GC_DEPTH);
    let mut bullshark = Bullshark::new(committee, store, metrics, NUM_SUB_DAGS_PER_SCHEDULE);

    let mut committed = false;
    for c in &certificates {
        let (outcome, sub_dags) = bullshark
            .process_certificate(&mut state, c.clone())
            .unwrap();

        // We expect leader of round 2 to not have enough support from certificates of round 3,
        // thus no commit should happen and every attempt should return a not enough support outcome
        if c.round() == 3 {
            assert_eq!(outcome, Outcome::NotEnoughSupportForLeader);
        }

        // We don't expect to have a leader at round 4, thus any addition of certificate on round 5
        // should not find a leader.
        if c.round() == 5 {
            assert_eq!(outcome, Outcome::LeaderNotFound);
        }

        // Leader election is triggered when populating certificates of odd rounds.
        if c.round() == 1 || c.round() == 2 || c.round() == 4 {
            assert_eq!(outcome, Outcome::NoLeaderElectedForOddRound);
        }

        // We do not expect any commit when inserting certificates below or equal round 6
        if c.round() <= 6 {
            assert_eq!(sub_dags.len(), 0);
        }

        // At round 7 we expect to trigger a commit
        if c.round() == 7 {
            match outcome {
                Outcome::NotEnoughSupportForLeader => {}
                Outcome::LeaderBelowCommitRound => {}
                Outcome::Commit => {
                    // we expect now to commit two sub dags, those with leader 6 and 2.
                    assert_eq!(sub_dags.len(), 2);

                    assert_eq!(sub_dags[0].leader.round(), 2);
                    assert_eq!(sub_dags[1].leader.round(), 6);

                    assert_eq!(sub_dags[0].certificates.len(), 4);
                    assert_eq!(sub_dags[1].certificates.len(), 10);

                    // And GC has collected everything up to round 5.
                    assert_eq!(state.dag.len(), 5);

                    for (round, entries) in state.dag.iter() {
                        assert!(*round >= 3, "{}", format!("Round detected: {}", round));

                        if *round == 3 || *round == 4 {
                            assert_eq!(entries.len(), 3);
                        } else if *round == 5 || *round == 6 {
                            assert_eq!(entries.len(), 4);
                        } else {
                            assert_eq!(entries.len(), 2);
                        }
                    }

                    committed = true;
                }
                _ => panic!("Unexpected outcome: {:?}", outcome),
            }
        }
    }

    assert!(committed);
}
