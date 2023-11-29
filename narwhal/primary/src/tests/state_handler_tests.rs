// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::consensus::LeaderSwapTable;
use fastcrypto::{
    serde_helpers::ToFromByteArray,
    traits::{KeyPair, ToFromBytes},
};

use test_utils::CommitteeFixture;

#[tokio::test]
async fn dkg() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder()
        .stake_distribution(vec![2500, 2500, 2500, 2500].into())
        .build();
    let committee = fixture.committee();

    let leader_schedule = LeaderSchedule::new(committee.clone(), LeaderSwapTable::default());
    let protocol_config = test_utils::latest_protocol_version();

    // RandomnessState elements for each authority.
    let mut vss_key_outputs = Vec::new();
    let mut randomness_states = Vec::new();
    let mut rx_system_messages_channels = Vec::new();

    for (_i, authority) in fixture.authorities().enumerate() {
        let network = test_utils::test_network(authority.network_keypair(), authority.address());
        let randomness_store = RandomnessStore::new_for_tests();
        let vss_key_output = Arc::new(OnceLock::new());

        let (tx_system_messages, rx_system_messages) = test_utils::test_channel!(1);

        let randomness_private_key = RandomnessPrivateKey::from(
            fastcrypto::groups::bls12381::Scalar::from_byte_array(
                authority
                    .keypair()
                    .copy()
                    .private()
                    .as_bytes()
                    .try_into()
                    .expect("key length should match"),
            )
            .expect("should work to convert BLS key to Scalar"),
        );
        let randomness_state = RandomnessState::try_new(
            &ChainIdentifier::unknown(),
            &protocol_config,
            committee.clone(),
            authority.id(),
            randomness_private_key,
            leader_schedule.clone(),
            network,
            vss_key_output.clone(),
            tx_system_messages,
            randomness_store,
        )
        .unwrap();

        vss_key_outputs.push(vss_key_output);
        rx_system_messages_channels.push(rx_system_messages);
        randomness_states.push(randomness_state);
    }

    // Generate and distribute Messages.
    type DkgG = <ThresholdBls12381MinSig as ThresholdBls>::Public;
    let mut dkg_messages = Vec::new();
    for i in 0..randomness_states.len() {
        randomness_states[i].start_dkg().await;

        let dkg_message = rx_system_messages_channels[i].recv().await.unwrap();
        match dkg_message {
            SystemMessage::DkgMessage(bytes) => {
                let msg: fastcrypto_tbls::dkg::Message<DkgG, DkgG> =
                    bcs::from_bytes(&bytes).expect("DKG message deserialization should not fail");
                assert_eq!(msg.sender, fixture.authority(i).id().0);
                dkg_messages.push(msg);
            }
            _ => panic!("wrong type of message sent"),
        }
    }
    for randomness_state in randomness_states.iter_mut() {
        for dkg_message in dkg_messages.iter().cloned() {
            randomness_state.add_message(dkg_message);
        }
        randomness_state.advance_dkg().await;
    }

    // Generate and distribute Confirmations.
    let mut dkg_confirmations = Vec::new();
    for (i, channel) in rx_system_messages_channels.iter_mut().enumerate() {
        let dkg_confirmation = channel.recv().await.unwrap();
        match dkg_confirmation {
            SystemMessage::DkgConfirmation(bytes) => {
                let conf: fastcrypto_tbls::dkg::Confirmation<DkgG> = bcs::from_bytes(&bytes)
                    .expect("DKG confirmation deserialization should not fail");
                assert_eq!(conf.sender, fixture.authority(i).id().0);
                dkg_confirmations.push(conf);
            }
            _ => panic!("wrong type of message sent"),
        }
    }
    for randomness_state in randomness_states.iter_mut() {
        for dkg_confirmation in dkg_confirmations.iter().cloned() {
            randomness_state.add_confirmation(dkg_confirmation);
        }
        randomness_state.advance_dkg().await;
    }

    // Verify DKG completed.
    for vss_key_output in vss_key_outputs {
        vss_key_output.get().expect("VSS key should be generated");
    }
}
