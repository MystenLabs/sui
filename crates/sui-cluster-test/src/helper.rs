// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use sui_core::gateway_state::GatewayClient;
use sui_json_rpc_types::{GetObjectDataResponse, SuiEvent};
use sui_types::gas_coin::GasCoin;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    event::TransferType,
    object::Owner,
};
use tracing::debug;

/// Verify Sui Coin exists with expected value and owner
/// When one of the verification fails, this function return an Error
/// instad of panicking.
pub async fn verify_gas_coin(
    client: &GatewayClient,
    obj_id: ObjectID,
    expected_owner: Owner,
    is_deleted: bool,
    is_sui_coin: bool,
) -> Result<Option<GasCoin>, anyhow::Error> {
    debug!(
        "Verifying object: {} (is {}a sui coin), owned by {}. Expect to be {}.",
        obj_id,
        if is_sui_coin { "" } else { "not " },
        expected_owner,
        if is_deleted { "deleted" } else { "alive" },
    );
    let object_id = obj_id;
    let object_info = client
        .get_object(object_id)
        .await
        .or_else(|err| bail!("Failed to get object info (id: {}), err: {err}", obj_id))?;
    match object_info {
        GetObjectDataResponse::NotExists(_) => {
            bail!("Gateway can't find gas object {}", object_id)
        }
        GetObjectDataResponse::Deleted(_) => {
            if !is_deleted {
                bail!("Gas object {} was deleted", object_id);
            }
            Ok(None)
        }
        GetObjectDataResponse::Exists(object) => {
            if is_deleted {
                panic!("Expect Gas object {} deleted, but it is not", object_id);
            }
            assert_eq!(
                object.owner, expected_owner,
                "Gas coin {} does not belong to {}, but {}",
                object_id, expected_owner, object.owner
            );
            if is_sui_coin {
                let move_obj = object
                    .data
                    .try_as_move()
                    .unwrap_or_else(|| panic!("Object {} is not a move object", object_id));

                let gas_coin = GasCoin::try_from(&move_obj.fields).unwrap_or_else(|err| {
                    panic!("Object {} is not a gas coin, {}", object_id, err)
                });
                return Ok(Some(gas_coin));
            }
            Ok(None)
        }
    }
}

#[macro_export]
macro_rules! assert_eq_if_present {
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if (left_val.is_none()) {
                } else if !(&left_val.as_ref().unwrap() == right_val) {
                    bail!("{} does not match, left: {:?}, right: {:?}", $($arg)+, left_val, right_val);
                }
            }
        }
    };
}

pub fn verify_transfer_object_event(
    event: &SuiEvent,
    e_package_id: Option<ObjectID>,
    e_transaction_module: Option<String>,
    e_sender: Option<SuiAddress>,
    e_recipient: Option<Owner>,
    e_object_id: Option<ObjectID>,
    e_version: Option<SequenceNumber>,
    e_type_: Option<TransferType>,
) -> Result<(), anyhow::Error> {
    if let SuiEvent::TransferObject {
        package_id,
        transaction_module,
        sender,
        recipient,
        object_id,
        version,
        type_,
    } = event
    {
        assert_eq_if_present!(e_package_id, package_id, "package_id");
        assert_eq_if_present!(
            e_transaction_module,
            transaction_module,
            "transaction_module"
        );
        assert_eq_if_present!(e_sender, sender, "sender");
        assert_eq_if_present!(e_recipient, recipient, "recipient");
        assert_eq_if_present!(e_object_id, object_id, "object_id");
        assert_eq_if_present!(e_version, version, "version");
        assert_eq_if_present!(e_type_, type_, "type_");
    }
    Ok(())
}
