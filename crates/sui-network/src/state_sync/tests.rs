// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    state_sync::{
        Builder, GetCheckpointSummaryRequest, StateSync, StateSyncMessage, UnstartedStateSync,
    },
    utils::build_network,
};
use anemo::{PeerId, Request};
use std::{collections::HashMap, time::Duration};
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, EpochId, StakeUnit},
    crypto::{
        AuthorityKeyPair, AuthoritySignInfo, AuthoritySignature, AuthorityWeakQuorumSignInfo,
        KeypairTraits, SuiAuthoritySignature,
    },
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointDigest, CheckpointSequenceNumber,
        CheckpointSummary, VerifiedCheckpoint,
    },
    storage::{ReadStore, SharedInMemoryStore, WriteStore},
};
use tokio::time::timeout;

#[tokio::test]
async fn server_push_checkpoint() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(1, None);
    let store = SharedInMemoryStore::default();

    let (
        UnstartedStateSync {
            handle: _handle,
            mut mailbox,
            peer_heights,
            ..
        },
        server,
    ) = Builder::new().store(store).build_internal();
    let peer_id = PeerId([9; 32]); // fake PeerId

    let checkpoint = ordered_checkpoints[0].inner().to_owned();
    let request = Request::new(checkpoint.clone()).with_extension(peer_id);
    server.push_checkpoint_summary(request).await.unwrap();

    assert_eq!(
        peer_heights.read().unwrap().heights.get(&peer_id),
        Some(&Some(0))
    );
    assert_eq!(
        peer_heights
            .read()
            .unwrap()
            .unprocessed_checkpoints
            .get(&checkpoint.digest())
            .unwrap()
            .summary,
        checkpoint.summary,
    );
    assert_eq!(
        peer_heights
            .read()
            .unwrap()
            .highest_known_checkpoint()
            .unwrap()
            .summary,
        checkpoint.summary,
    );
    assert!(matches!(
        mailbox.try_recv().unwrap(),
        StateSyncMessage::StartSyncJob
    ));
}

#[tokio::test]
async fn server_get_checkpoint() {
    let (builder, server) = Builder::new()
        .store(SharedInMemoryStore::default())
        .build_internal();

    // Requests for checkpoints that aren't in the server's store
    let requests = [
        GetCheckpointSummaryRequest::Latest,
        GetCheckpointSummaryRequest::BySequenceNumber(9),
        GetCheckpointSummaryRequest::ByDigest([10; 32]),
    ];
    for request in requests {
        let response = server
            .get_checkpoint_summary(Request::new(request))
            .await
            .unwrap()
            .into_inner();
        assert!(response.is_none());
    }

    // Populate the node's store with some checkpoints
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(3, None);
    for checkpoint in ordered_checkpoints.clone() {
        builder.store.inner_mut().insert_checkpoint(checkpoint)
    }
    let latest = ordered_checkpoints.last().unwrap().clone();
    builder
        .store
        .inner_mut()
        .update_highest_synced_checkpoint(&latest);

    let request = Request::new(GetCheckpointSummaryRequest::Latest);
    let response = server
        .get_checkpoint_summary(request)
        .await
        .unwrap()
        .into_inner()
        .unwrap();
    assert_eq!(response.summary, latest.summary);

    for checkpoint in ordered_checkpoints {
        let request = Request::new(GetCheckpointSummaryRequest::ByDigest(checkpoint.digest()));
        let response = server
            .get_checkpoint_summary(request)
            .await
            .unwrap()
            .into_inner()
            .unwrap();
        assert_eq!(response.summary, checkpoint.summary);

        let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(
            checkpoint.sequence_number(),
        ));
        let response = server
            .get_checkpoint_summary(request)
            .await
            .unwrap()
            .into_inner()
            .unwrap();
        assert_eq!(response.summary, checkpoint.summary);
    }
}

#[tokio::test]
async fn isolated_sync_job() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);

    // Build and connect two nodes
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (mut event_loop_1, _handle_1) = builder.build(network_1.clone());
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone());
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Init the root committee in both nodes
    event_loop_1
        .store
        .inner_mut()
        .insert_committee(committee.committee().to_owned());
    event_loop_2
        .store
        .inner_mut()
        .insert_committee(committee.committee().to_owned());

    // build mock data
    let (ordered_checkpoints, sequence_number_to_digest, checkpoints) =
        committee.make_checkpoints(100, None);

    // Node 2 will have all the data
    {
        let mut store = event_loop_2.store.inner_mut();
        for checkpoint in ordered_checkpoints.clone() {
            store.insert_checkpoint(checkpoint);
        }
    }

    // Node 1 will know that Node 2 has the data
    event_loop_1
        .peer_heights
        .write()
        .unwrap()
        .update_peer_height(
            network_2.peer_id(),
            ordered_checkpoints
                .last()
                .cloned()
                .map(VerifiedCheckpoint::into_inner),
        );

    // Sync the data
    event_loop_1.maybe_start_checkpoint_summary_sync_task();
    event_loop_1.tasks.join_next().await.unwrap().unwrap();
    assert_eq!(
        ordered_checkpoints.last().map(|x| &x.summary),
        event_loop_1
            .store
            .get_highest_verified_checkpoint()
            .unwrap()
            .as_ref()
            .map(|x| &x.summary)
    );

    {
        let store = event_loop_1.store.inner();
        let expected = checkpoints
            .iter()
            .map(|(key, value)| (key, &value.summary))
            .collect::<HashMap<_, _>>();
        let actual = store
            .checkpoints()
            .iter()
            .map(|(key, value)| (key, &value.summary))
            .collect::<HashMap<_, _>>();
        assert_eq!(actual, expected);
        assert_eq!(
            store.checkpoint_sequence_number_to_digest(),
            &sequence_number_to_digest
        );
    }
}

#[tokio::test]
async fn sync_with_checkpoints_being_inserted() {
    telemetry_subscribers::init_for_testing();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);

    // Build and connect two nodes
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_1, handle_1) = builder.build(network_1.clone());
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_2, handle_2) = builder.build(network_2.clone());
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Init the root committee in both nodes
    event_loop_1
        .store
        .inner_mut()
        .insert_committee(committee.committee().to_owned());
    event_loop_2
        .store
        .inner_mut()
        .insert_committee(committee.committee().to_owned());

    // get handles to each node's stores
    let store_1 = event_loop_1.store.clone();
    let store_2 = event_loop_2.store.clone();
    // make sure that node_1 knows about node_2
    event_loop_1
        .peer_heights
        .write()
        .unwrap()
        .heights
        .insert(network_2.peer_id(), None);
    // Start both event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    // build mock data
    let (ordered_checkpoints, sequence_number_to_digest, checkpoints) =
        committee.make_checkpoints(4, None);

    let mut subscriber_1 = handle_1.subscribe_to_synced_checkpoints();
    let mut subscriber_2 = handle_2.subscribe_to_synced_checkpoints();

    // Inject one checkpoint and verify that it was shared with the other node
    let mut checkpoint_iter = ordered_checkpoints.clone().into_iter();
    store_1
        .insert_checkpoint_contents(empty_contents())
        .unwrap();
    handle_1
        .send_checkpoint(checkpoint_iter.next().unwrap())
        .await;

    timeout(Duration::from_secs(1), async {
        assert_eq!(
            subscriber_1.recv().await.unwrap().summary(),
            ordered_checkpoints[0].summary(),
        );
        assert_eq!(
            subscriber_2.recv().await.unwrap().summary(),
            ordered_checkpoints[0].summary()
        );
    })
    .await
    .unwrap();

    // Inject all the checkpoints
    for checkpoint in checkpoint_iter {
        handle_1.send_checkpoint(checkpoint).await;
    }

    timeout(Duration::from_secs(1), async {
        for checkpoint in &ordered_checkpoints[1..] {
            assert_eq!(
                subscriber_1.recv().await.unwrap().summary(),
                checkpoint.summary()
            );
            assert_eq!(
                subscriber_2.recv().await.unwrap().summary(),
                checkpoint.summary()
            );
        }
    })
    .await
    .unwrap();

    let store_1 = store_1.inner();
    let store_2 = store_2.inner();
    assert_eq!(
        ordered_checkpoints.last().map(|x| x.digest()),
        store_1
            .get_highest_verified_checkpoint()
            .as_ref()
            .map(|x| x.digest())
    );
    assert_eq!(
        ordered_checkpoints.last().map(|x| x.digest()),
        store_2
            .get_highest_verified_checkpoint()
            .as_ref()
            .map(|x| x.digest())
    );

    let expected = checkpoints
        .iter()
        .map(|(key, value)| (key, &value.summary))
        .collect::<HashMap<_, _>>();
    let actual_1 = store_1
        .checkpoints()
        .iter()
        .map(|(key, value)| (key, &value.summary))
        .collect::<HashMap<_, _>>();
    assert_eq!(actual_1, expected);
    assert_eq!(
        store_1.checkpoint_sequence_number_to_digest(),
        &sequence_number_to_digest
    );

    let actual_2 = store_2
        .checkpoints()
        .iter()
        .map(|(key, value)| (key, &value.summary))
        .collect::<HashMap<_, _>>();
    assert_eq!(actual_2, expected);
    assert_eq!(
        store_2.checkpoint_sequence_number_to_digest(),
        &sequence_number_to_digest
    );
}

struct CommitteeFixture {
    epoch: EpochId,
    validators: HashMap<AuthorityName, (AuthorityKeyPair, StakeUnit)>,
    committee: Committee,
}

impl CommitteeFixture {
    pub fn generate<R: ::rand::RngCore + ::rand::CryptoRng>(
        mut rng: R,
        epoch: EpochId,
        committee_size: usize,
    ) -> Self {
        let validators = (0..committee_size)
            .map(|_| sui_types::crypto::get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut rng).1)
            .map(|keypair| (keypair.public().into(), (keypair, 1)))
            .collect::<HashMap<_, _>>();

        let committee = Committee::new(
            epoch,
            validators
                .iter()
                .map(|(name, (_, stake))| (*name, *stake))
                .collect(),
        )
        .unwrap();

        Self {
            epoch,
            validators,
            committee,
        }
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    fn create_root_checkpoint(&self) -> VerifiedCheckpoint {
        assert_eq!(self.epoch, 0, "root checkpoint must be epoch 0");
        let checkpoint = CheckpointSummary {
            epoch: 0,
            sequence_number: 0,
            content_digest: empty_contents().digest(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: Default::default(),
            next_epoch_committee: None,
        };

        self.create_certified_checkpoint(checkpoint)
    }

    fn create_certified_checkpoint(&self, checkpoint: CheckpointSummary) -> VerifiedCheckpoint {
        let signatures = self
            .validators
            .iter()
            .map(|(name, (key, _))| {
                let signature = AuthoritySignature::new(&checkpoint, checkpoint.epoch, key);
                AuthoritySignInfo {
                    epoch: checkpoint.epoch,
                    authority: *name,
                    signature,
                }
            })
            .collect();

        let checkpoint = CertifiedCheckpointSummary {
            summary: checkpoint,
            auth_signature: AuthorityWeakQuorumSignInfo::new_from_auth_sign_infos(
                signatures,
                self.committee(),
            )
            .unwrap(),
        };

        let checkpoint = VerifiedCheckpoint::new(checkpoint, self.committee()).unwrap();

        checkpoint
    }

    pub fn make_checkpoints(
        &self,
        number_of_checkpoints: usize,
        previous_checkpoint: Option<VerifiedCheckpoint>,
    ) -> (
        Vec<VerifiedCheckpoint>,
        HashMap<CheckpointSequenceNumber, CheckpointDigest>,
        HashMap<CheckpointDigest, VerifiedCheckpoint>,
    ) {
        // Only skip the first one if it was supplied
        let skip = previous_checkpoint.is_some() as usize;
        let first = previous_checkpoint.unwrap_or_else(|| self.create_root_checkpoint());

        let ordered_checkpoints = std::iter::successors(Some(first), |prev| {
            let summary = CheckpointSummary {
                epoch: self.epoch,
                sequence_number: prev.summary.sequence_number + 1,
                content_digest: empty_contents().digest(),
                previous_digest: Some(prev.summary.digest()),
                epoch_rolling_gas_cost_summary: Default::default(),
                next_epoch_committee: None,
            };

            let checkpoint = self.create_certified_checkpoint(summary);

            Some(checkpoint)
        })
        .skip(skip)
        .take(number_of_checkpoints)
        .collect::<Vec<_>>();

        let (sequence_number_to_digest, checkpoints) = ordered_checkpoints
            .iter()
            .cloned()
            .map(|checkpoint| {
                let digest = checkpoint.summary.digest();
                (
                    (checkpoint.summary.sequence_number, digest),
                    (digest, checkpoint),
                )
            })
            .unzip();

        (ordered_checkpoints, sequence_number_to_digest, checkpoints)
    }
}

pub fn empty_contents() -> CheckpointContents {
    CheckpointContents::new_with_causally_ordered_transactions(std::iter::empty())
}
