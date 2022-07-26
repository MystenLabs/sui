// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use multiaddr::Multiaddr;
use std::sync::Arc;
use std::time::Duration;
use sui_config::ValidatorInfo;
use sui_core::authority::AuthorityState;
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_core::checkpoints::{CheckpointLocals, CHECKPOINT_COUNT_PER_EPOCH};
use sui_core::safe_client::SafeClient;
use sui_node::SuiNode;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::crypto::{get_key_pair, KeypairTraits};
use sui_types::messages::ObjectInfoResponse;
use sui_types::messages::{CallArg, ObjectArg, ObjectInfoRequest, TransactionEffects};
use sui_types::object::Object;
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
use test_utils::authority::test_authority_configs;
use test_utils::messages::move_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::transaction::submit_single_owner_transaction;

#[tokio::test]
async fn test_epoch_change_committee_updates() {
    let mut configs = test_authority_configs();
    for c in configs.validator_configs.iter_mut() {
        c.enable_gossip = true;
    }
    let _validator_info = configs.validator_set();
    let gas_objects = test_gas_objects();
    let mut states = Vec::new();
    let mut nodes = Vec::new();
    for validator in configs.validator_configs() {
        let node = SuiNode::start(validator).await.unwrap();
        let state = node.state();

        state
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .set_locals_for_testing(CheckpointLocals {
                next_checkpoint: CHECKPOINT_COUNT_PER_EPOCH,
                proposal_next_transaction: None,
                next_transaction_sequence: 0,
                no_more_fragments: true,
                current_proposal: None,
            })
            .unwrap();

        for gas in gas_objects.clone() {
            state.insert_genesis_object(gas).await;
        }
        states.push(state);
        nodes.push(node);
    }

    let _sui_system_state_ref = states[0].get_sui_system_state_object_ref().await.unwrap();

    // get sui system state and confirm it matches network info
    let sui_system_state = states[0].get_sui_system_state_object().await.unwrap();
    let mut net_addrs_from_chain: Vec<Multiaddr> = Vec::new();
    for validator in sui_system_state.validators.active_validators {
        let address = Multiaddr::try_from(validator.metadata.net_address);
        net_addrs_from_chain.push(address.unwrap());
    }
    let mut net_addrs_from_config = Vec::new();
    for validator in configs.validator_configs() {
        net_addrs_from_config.push(validator.network_address.clone());
    }
    assert_eq!(net_addrs_from_config.len(), net_addrs_from_chain.len());
    net_addrs_from_config.sort();
    net_addrs_from_chain.sort();
    for (conf, chain) in net_addrs_from_config.iter().zip(&net_addrs_from_chain) {
        assert_eq!(conf, chain);
    }

    for node in nodes {
        let active = node.active().unwrap();
        active.start_epoch_change().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        node.state()
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .set_locals_for_testing(CheckpointLocals {
                next_checkpoint: CHECKPOINT_COUNT_PER_EPOCH + 1,
                proposal_next_transaction: None,
                next_transaction_sequence: 0,
                no_more_fragments: true,
                current_proposal: None,
            })
            .unwrap();

        active.finish_epoch_change().await.unwrap();
    }

    // refresh the system state and network addresses
    let sui_system_state = states[0].get_sui_system_state_object().await.unwrap();
    assert_eq!(sui_system_state.epoch, 1);

    let mut net_addrs_from_chain: Vec<Multiaddr> = Vec::new();
    for validator in sui_system_state.validators.active_validators {
        let address = Multiaddr::try_from(validator.metadata.net_address);
        net_addrs_from_chain.push(address.unwrap());
    }
    let mut net_addrs_from_config = Vec::new();
    for validator in configs.validator_configs() {
        net_addrs_from_config.push(validator.network_address.clone());
    }

    // ensure they still match the original
    assert_eq!(net_addrs_from_config.len(), net_addrs_from_chain.len());
    net_addrs_from_config.sort();
    net_addrs_from_chain.sort();
    for (conf, chain) in net_addrs_from_config.iter().zip(&net_addrs_from_chain) {
        assert_eq!(conf, chain);
    }
}

#[allow(dead_code)]
pub async fn create_and_register_new_validator(
    state: Arc<AuthorityState>,
    node: SuiNode,
    mut gas_objects: Vec<Object>,
    validator_info: &[ValidatorInfo],
) -> TransactionEffects {
    let new_validator = get_new_validator();
    let package = state.get_framework_object_ref().await.unwrap();
    let authority = node.active().unwrap().net.load();
    let authority_client: &SafeClient<NetworkAuthorityClient> =
        authority.authority_clients.values().last().unwrap();
    let gas_obj = get_latest_ref(authority_client, gas_objects.pop().unwrap().id()).await;

    let validator_tx = move_transaction(
        gas_objects.pop().unwrap(),
        "sui_system",
        "request_add_validator",
        package,
        vec![
            CallArg::Object(ObjectArg::SharedObject(SUI_SYSTEM_STATE_OBJECT_ID)),
            CallArg::Pure(bcs::to_bytes(&new_validator.public_key()).unwrap()),
            CallArg::Pure(
                bcs::to_bytes(format!("Validator{}", new_validator.sui_address()).as_bytes())
                    .unwrap(),
            ),
            CallArg::Pure(bcs::to_bytes(&new_validator.network_address).unwrap()),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(gas_obj)),
        ],
    );
    submit_single_owner_transaction(validator_tx, validator_info).await
}

#[allow(dead_code)]
pub fn get_new_validator() -> ValidatorInfo {
    let keypair = get_key_pair();
    ValidatorInfo {
        name: "".to_string(),
        public_key: keypair.1.public().into(),
        stake: 1,
        delegation: 0,
        network_address: sui_config::utils::new_network_address(),
        narwhal_primary_to_primary: sui_config::utils::new_network_address(),
        narwhal_worker_to_primary: sui_config::utils::new_network_address(),
        narwhal_primary_to_worker: sui_config::utils::new_network_address(),
        narwhal_worker_to_worker: sui_config::utils::new_network_address(),
        narwhal_consensus_address: sui_config::utils::new_network_address(),
    }
}

#[allow(dead_code)]
pub async fn get_latest_ref<A>(authority: &SafeClient<A>, object_id: ObjectID) -> ObjectRef
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    if let Ok(ObjectInfoResponse {
        requested_object_reference: Some(object_ref),
        ..
    }) = authority
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id, None,
        ))
        .await
    {
        return object_ref;
    }
    panic!("Object not found!");
}
