// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::base_types::TransactionDigest;
use crate::object::{MoveObject, Owner, OBJECT_START_VERSION};
use crate::storage::{DeleteKind, SingleTxContext, WriteKind};
use crate::temporary_store::TemporaryStore;
use crate::{
    balance::{Balance, Supply},
    error::{ExecutionError, ExecutionErrorKind},
    object::{Data, Object},
};
use crate::{
    base_types::{ObjectID, SuiAddress},
    id::UID,
    SUI_FRAMEWORK_ADDRESS,
};
use schemars::JsonSchema;

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("coin");
pub const COIN_STRUCT_NAME: &IdentStr = ident_str!("Coin");

pub const PAY_MODULE_NAME: &IdentStr = ident_str!("pay");
pub const PAY_JOIN_FUNC_NAME: &IdentStr = ident_str!("join");
pub const PAY_SPLIT_N_FUNC_NAME: &IdentStr = ident_str!("divide_and_keep");
pub const PAY_SPLIT_VEC_FUNC_NAME: &IdentStr = ident_str!("split_vec");

// Rust version of the Move sui::coin::Coin type
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Eq, PartialEq)]
pub struct Coin {
    pub id: UID,
    pub balance: Balance,
}

impl Coin {
    pub fn new(id: UID, value: u64) -> Self {
        Self {
            id,
            balance: Balance::new(value),
        }
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: COIN_STRUCT_NAME.to_owned(),
            module: COIN_MODULE_NAME.to_owned(),
            type_params: vec![TypeTag::Struct(type_param)],
        }
    }

    /// Is this other StructTag representing a Coin?
    pub fn is_coin(other: &StructTag) -> bool {
        other.module.as_ident_str() == COIN_MODULE_NAME
            && other.name.as_ident_str() == COIN_STRUCT_NAME
    }

    /// Create a coin from BCS bytes
    pub fn from_bcs_bytes(content: &[u8]) -> Result<Self, ExecutionError> {
        bcs::from_bytes(content).map_err(|err| {
            ExecutionError::new_with_source(
                ExecutionErrorKind::InvalidCoinObject,
                format!("Unable to deserialize coin object: {:?}", err),
            )
        })
    }

    /// If the given object is a Coin, deserialize its contents and extract the balance Ok(Some(u64)).
    /// If it's not a Coin, return Ok(None).
    /// The cost is 2 comparisons if not a coin, and deserialization if its a Coin.
    pub fn extract_balance_if_coin(object: &Object) -> Result<Option<u64>, ExecutionError> {
        match &object.data {
            Data::Move(move_obj) => {
                if !Self::is_coin(&move_obj.type_) {
                    return Ok(None);
                }

                let coin = Self::from_bcs_bytes(move_obj.contents())?;
                Ok(Some(coin.value()))
            }
            _ => Ok(None), // package
        }
    }

    pub fn id(&self) -> &ObjectID {
        self.id.object_id()
    }

    pub fn value(&self) -> u64 {
        self.balance.value()
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout(type_param: StructTag) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(type_param.clone()),
            fields: vec![
                MoveFieldLayout::new(
                    ident_str!("id").to_owned(),
                    MoveTypeLayout::Struct(UID::layout()),
                ),
                MoveFieldLayout::new(
                    ident_str!("balance").to_owned(),
                    MoveTypeLayout::Struct(Balance::layout(type_param)),
                ),
            ],
        }
    }

    // Shift balance of coins_to_merge to this coin.
    // Related coin objects need to be updated in temporary_store to presist the changes,
    // including deleting the coin objects that have been merged.
    pub fn merge_coins(&mut self, coins_to_merge: &mut [Coin]) {
        let total_coins = coins_to_merge.iter().fold(0, |acc, c| acc + c.value());
        for coin in coins_to_merge.iter_mut() {
            // unwrap() is safe because balance value is the same as coin value
            coin.balance.withdraw(coin.value()).unwrap();
        }
        self.balance = Balance::new(self.value() + total_coins);
    }

    // Split amount out of this coin to a new coin.
    // Related coin objects need to be updated in temporary_store to presist the changes,
    // including creating the coin object related to the newly created coin.
    pub fn split_coin(&mut self, amount: u64, new_coin_id: UID) -> Result<Coin, ExecutionError> {
        self.balance.withdraw(amount)?;
        Ok(Coin::new(new_coin_id, amount))
    }
}

// Rust version of the Move sui::coin::TreasuryCap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TreasuryCap {
    pub id: UID,
    pub total_supply: Supply,
}

pub fn transfer_coin<S>(
    ctx: &SingleTxContext,
    temporary_store: &mut TemporaryStore<S>,
    coin: &Coin,
    recipient: SuiAddress,
    coin_type: StructTag,
    previous_transaction: TransactionDigest,
) {
    let new_coin = Object::new_move(
        MoveObject::new_coin(
            coin_type,
            OBJECT_START_VERSION,
            bcs::to_bytes(coin).expect("Serializing coin value cannot fail"),
        ),
        Owner::AddressOwner(recipient),
        previous_transaction,
    );
    temporary_store.write_object(ctx, new_coin, WriteKind::Create);
}

// A helper function for pay_sui and pay_all_sui.
// It updates the gas_coin_obj based on the updated gas_coin, transfers gas_coin_obj to
// recipient when needed, and then deletes all other input coins other than gas_coin_obj.
pub fn update_input_coins<S>(
    ctx: &SingleTxContext,
    temporary_store: &mut TemporaryStore<S>,
    coin_objects: &mut Vec<Object>,
    gas_coin: &Coin,
    recipient: Option<SuiAddress>,
) {
    let mut gas_coin_obj = coin_objects.remove(0);
    // unwrap is safe because we checked that it was a coin object above.
    // update_contents_without_version_change b/c this is the gas coin,
    // whose version will be bumped upon gas payment.
    gas_coin_obj
        .data
        .try_as_move_mut()
        .unwrap()
        .update_contents_without_version_change(
            bcs::to_bytes(gas_coin).expect("Coin serialization should not fail"),
        );
    if let Some(recipient) = recipient {
        gas_coin_obj.transfer_without_version_change(recipient);
    }
    temporary_store.write_object(ctx, gas_coin_obj, WriteKind::Mutate);

    for coin_object in coin_objects.iter() {
        temporary_store.delete_object(
            ctx,
            &coin_object.id(),
            coin_object.version(),
            DeleteKind::Normal,
        )
    }
}
