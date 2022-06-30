// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::ValidatorInfo;
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_core::safe_client::SafeClient;
use sui_node::SuiNode;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::crypto::get_key_pair;
use sui_types::messages::{CallArg, ObjectArg, ObjectInfoRequest};
use sui_types::messages::{ExecutionStatus, ObjectInfoResponse};
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
    let validator_info = configs.validator_set();
    let mut gas_objects = test_gas_objects();
    let mut states = Vec::new();
    let mut handles = Vec::new();
    for validator in configs.validator_configs() {
        let node = SuiNode::start(validator).await.unwrap();
        let state = node.state();
        for gas in gas_objects.clone() {
            state.insert_genesis_object(gas).await;
        }
        //node.active().unwrap().start_epoch_change();
        states.push(state);
        handles.push(node);
    }

    let _sui_system_state_ref = states[0].get_sui_system_state_object_ref().await.unwrap();

    let new_validator = get_new_validator();
    let package = states[0].get_framework_object_ref().await.unwrap();
    let authority = handles[0].active().unwrap().net.load();
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
    let validator_effects = submit_single_owner_transaction(validator_tx, validator_info).await;
    let _k = 1;

    assert!(matches!(
        validator_effects.status,
        ExecutionStatus::Success { .. }
    ));

    let _transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "sui_system",
        "request_add_validator",
        gas_objects.pop().unwrap().compute_object_reference(),
        vec![], // TODO
    );
    // todo: get sui system state and confirm it matches network info
    assert_eq!(1, 1);
}

pub fn get_new_validator() -> ValidatorInfo {
    let keypair = get_key_pair();
    ValidatorInfo {
        name: "".to_string(),
        public_key: *keypair.1.public_key_bytes(),
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
