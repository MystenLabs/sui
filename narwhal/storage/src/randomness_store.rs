// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::NodeStorage;

use fastcrypto::groups;
use fastcrypto_tbls::dkg;
use fastcrypto_tbls::nodes::PartyId;
use store::reopen;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use store::{rocks::DBMap, Map};
use sui_macros::fail_point;
use types::RandomnessRound;

pub(crate) type PkG = groups::bls12381::G2Element;
pub(crate) type EncG = groups::bls12381::G2Element;

pub(crate) type SingletonKey = u32;
pub(crate) const SINGLETON_KEY: SingletonKey = 0;

/// The storage for the last votes digests per authority
#[derive(Clone)]
pub struct RandomnessStore {
    processed_messages: DBMap<PartyId, dkg::ProcessedMessage<PkG, EncG>>,
    used_messages: DBMap<SingletonKey, dkg::UsedProcessedMessages<PkG, EncG>>,
    confirmations: DBMap<PartyId, dkg::Confirmation<EncG>>,
    dkg_output: DBMap<SingletonKey, dkg::Output<PkG, EncG>>,
    randomness_round: DBMap<SingletonKey, RandomnessRound>,
}

impl RandomnessStore {
    pub fn new(
        processed_messages: DBMap<PartyId, dkg::ProcessedMessage<PkG, EncG>>,
        used_messages: DBMap<SingletonKey, dkg::UsedProcessedMessages<PkG, EncG>>,
        confirmations: DBMap<PartyId, dkg::Confirmation<EncG>>,
        dkg_output: DBMap<SingletonKey, dkg::Output<PkG, EncG>>,
        randomness_round: DBMap<SingletonKey, RandomnessRound>,
    ) -> RandomnessStore {
        Self {
            processed_messages,
            used_messages,
            confirmations,
            dkg_output,
            randomness_round,
        }
    }

    pub fn new_for_tests() -> RandomnessStore {
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[
                NodeStorage::PROCESSED_MESSAGES_CF,
                NodeStorage::USED_MESSAGES_CF,
                NodeStorage::CONFIRMATIONS_CF,
                NodeStorage::DKG_OUTPUT_CF,
                NodeStorage::RANDOMNESS_ROUND_CF,
            ],
        )
        .expect("database open should not fail");
        let (processed_messages, used_messages, confirmations, dkg_output, randomness_round) = reopen!(
            &rocksdb,
            NodeStorage::PROCESSED_MESSAGES_CF;<PartyId, dkg::ProcessedMessage<PkG, EncG>>,
            NodeStorage::USED_MESSAGES_CF;<SingletonKey, dkg::UsedProcessedMessages<PkG, EncG>>,
            NodeStorage::CONFIRMATIONS_CF;<PartyId, dkg::Confirmation<EncG>>,
            NodeStorage::DKG_OUTPUT_CF;<SingletonKey, dkg::Output<PkG, EncG>>,
            NodeStorage::RANDOMNESS_ROUND_CF;<SingletonKey, RandomnessRound>
        );
        Self::new(
            processed_messages,
            used_messages,
            confirmations,
            dkg_output,
            randomness_round,
        )
    }

    pub fn add_processed_message(
        &self,
        party_id: PartyId,
        processed_message: dkg::ProcessedMessage<PkG, EncG>,
    ) {
        fail_point!("narwhal-store-before-write");

        self.processed_messages
            .insert(&party_id, &processed_message)
            .expect("typed_store should not fail");

        fail_point!("narwhal-store-after-write");
    }

    pub fn processed_messages(&self) -> Vec<dkg::ProcessedMessage<PkG, EncG>> {
        self.processed_messages
            .safe_iter()
            .map(|result| result.expect("typed_store should not fail").1)
            .collect()
    }

    pub fn set_used_messages(&self, used_messages: dkg::UsedProcessedMessages<PkG, EncG>) {
        fail_point!("narwhal-store-before-write");

        self.used_messages
            .insert(&SINGLETON_KEY, &used_messages)
            .expect("typed_store should not fail");

        fail_point!("narwhal-store-after-write");
    }

    pub fn used_messages(&self) -> Option<dkg::UsedProcessedMessages<PkG, EncG>> {
        self.used_messages
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail")
    }

    pub fn has_used_messages(&self) -> bool {
        self.used_messages
            .contains_key(&SINGLETON_KEY)
            .expect("typed_store should not fail")
    }

    pub fn add_confirmation(&self, party_id: PartyId, confirmation: dkg::Confirmation<EncG>) {
        fail_point!("narwhal-store-before-write");

        self.confirmations
            .insert(&party_id, &confirmation)
            .expect("typed_store should not fail");

        fail_point!("narwhal-store-after-write");
    }

    pub fn confirmations(&self) -> Vec<dkg::Confirmation<EncG>> {
        self.confirmations
            .safe_iter()
            .map(|result| result.expect("typed_store should not fail").1)
            .collect()
    }

    pub fn set_dkg_output(&self, dkg_output: dkg::Output<PkG, EncG>) {
        fail_point!("narwhal-store-before-write");

        self.dkg_output
            .insert(&SINGLETON_KEY, &dkg_output)
            .expect("typed_store should not fail");

        fail_point!("narwhal-store-after-write");
    }

    pub fn dkg_output(&self) -> Option<dkg::Output<PkG, EncG>> {
        self.dkg_output
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail")
    }

    pub fn has_dkg_output(&self) -> bool {
        self.dkg_output
            .contains_key(&SINGLETON_KEY)
            .expect("typed_store should not fail")
    }

    pub fn set_randomness_round(&self, randomness_round: RandomnessRound) {
        fail_point!("narwhal-store-before-write");

        self.randomness_round
            .insert(&SINGLETON_KEY, &randomness_round)
            .expect("typed_store should not fail");

        fail_point!("narwhal-store-after-write");
    }

    pub fn randomness_round(&self) -> RandomnessRound {
        self.randomness_round
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail")
            .unwrap_or(RandomnessRound(0))
    }
}
