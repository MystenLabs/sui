// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::object::{MoveObject, Owner};
use crate::storage::WriteKind;
use crate::temporary_store::TemporaryStore;
use crate::{
    balance::{Balance, Supply},
    error::ExecutionError,
    object::{Data, Object},
};
use crate::{base_types::TransactionDigest, error::SuiError};
use crate::{
    base_types::{MoveObjectType, SequenceNumber},
    error::ExecutionErrorKind,
};
use crate::{
    base_types::{ObjectID, SuiAddress},
    id::UID,
    SUI_FRAMEWORK_ADDRESS,
};
use schemars::JsonSchema;

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("coin");
pub const COIN_STRUCT_NAME: &IdentStr = ident_str!("Coin");
pub const COIN_METADATA_STRUCT_NAME: &IdentStr = ident_str!("CoinMetadata");
pub const COIN_TREASURE_CAP_NAME: &IdentStr = ident_str!("TreasuryCap");

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

    pub fn type_(type_param: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: COIN_STRUCT_NAME.to_owned(),
            module: COIN_MODULE_NAME.to_owned(),
            type_params: vec![type_param],
        }
    }

    /// Is this other StructTag representing a Coin?
    pub fn is_coin(other: &StructTag) -> bool {
        other.address == SUI_FRAMEWORK_ADDRESS
            && other.module.as_ident_str() == COIN_MODULE_NAME
            && other.name.as_ident_str() == COIN_STRUCT_NAME
    }

    /// Create a coin from BCS bytes
    pub fn from_bcs_bytes(content: &[u8]) -> Result<Self, bcs::Error> {
        bcs::from_bytes(content)
    }

    /// If the given object is a Coin, deserialize its contents and extract the balance Ok(Some(u64)).
    /// If it's not a Coin, return Ok(None).
    /// The cost is 2 comparisons if not a coin, and deserialization if its a Coin.
    pub fn extract_balance_if_coin(object: &Object) -> Result<Option<u64>, bcs::Error> {
        match &object.data {
            Data::Move(move_obj) => {
                if !move_obj.is_coin() {
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

    pub fn layout(type_param: TypeTag) -> MoveStructLayout {
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

    /// Add balance to this coin, erroring if the new total balance exceeds the maximum
    pub fn add(&mut self, balance: Balance) -> Result<(), ExecutionError> {
        let Some(new_value) = self.value().checked_add(balance.value())
        else {
            return Err(ExecutionError::from_kind(
                ExecutionErrorKind::CoinBalanceOverflow,
            ));
        };
        self.balance = Balance::new(new_value);
        Ok(())
    }

    // Split amount out of this coin to a new coin.
    // Related coin objects need to be updated in temporary_store to persist the changes,
    // including creating the coin object related to the newly created coin.
    pub fn split(&mut self, amount: u64, new_coin_id: UID) -> Result<Coin, ExecutionError> {
        self.balance.withdraw(amount)?;
        Ok(Coin::new(new_coin_id, amount))
    }
}

// Rust version of the Move sui::coin::TreasuryCap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct TreasuryCap {
    pub id: UID,
    pub total_supply: Supply,
}

impl TreasuryCap {
    /// Create a TreasuryCap from BCS bytes
    pub fn from_bcs_bytes(content: &[u8]) -> Result<Self, SuiError> {
        bcs::from_bytes(content).map_err(|err| SuiError::ObjectDeserializationError {
            error: format!("Unable to deserialize TreasuryCap object: {}", err),
        })
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: COIN_TREASURE_CAP_NAME.to_owned(),
            module: COIN_MODULE_NAME.to_owned(),
            type_params: vec![TypeTag::Struct(Box::new(type_param))],
        }
    }
}

pub fn transfer_coin(
    temporary_store: &mut TemporaryStore<'_>,
    coin: &Coin,
    recipient: SuiAddress,
    coin_type: MoveObjectType,
    previous_transaction: TransactionDigest,
) {
    let new_coin = Object::new_move(
        MoveObject::new_coin(coin_type, SequenceNumber::new(), *coin.id(), coin.value()),
        Owner::AddressOwner(recipient),
        previous_transaction,
    );
    temporary_store.write_object(new_coin, WriteKind::Create);
}

// Rust version of the Move sui::coin::CoinMetadata type
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Eq, PartialEq)]
pub struct CoinMetadata {
    pub id: UID,
    /// Number of decimal places the coin uses.
    pub decimals: u8,
    /// Name for the token
    pub name: String,
    /// Symbol for the token
    pub symbol: String,
    /// Description of the token
    pub description: String,
    /// URL for the token logo
    pub icon_url: Option<String>,
}

impl CoinMetadata {
    /// Is this other StructTag representing a CoinMetadata?
    pub fn is_coin_metadata(other: &StructTag) -> bool {
        other.address == SUI_FRAMEWORK_ADDRESS
            && other.module.as_ident_str() == COIN_MODULE_NAME
            && other.name.as_ident_str() == COIN_METADATA_STRUCT_NAME
    }

    /// Create a coin from BCS bytes
    pub fn from_bcs_bytes(content: &[u8]) -> Result<Self, SuiError> {
        bcs::from_bytes(content).map_err(|err| SuiError::ObjectDeserializationError {
            error: format!("Unable to deserialize CoinMetadata object: {}", err),
        })
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: COIN_METADATA_STRUCT_NAME.to_owned(),
            module: COIN_MODULE_NAME.to_owned(),
            type_params: vec![TypeTag::Struct(Box::new(type_param))],
        }
    }
}

impl TryFrom<Object> for CoinMetadata {
    type Error = SuiError;
    fn try_from(object: Object) -> Result<Self, Self::Error> {
        TryFrom::try_from(&object)
    }
}

impl TryFrom<&Object> for CoinMetadata {
    type Error = SuiError;
    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        match &object.data {
            Data::Move(o) => {
                if o.type_().is_coin_metadata() {
                    return CoinMetadata::from_bcs_bytes(o.contents());
                }
            }
            Data::Package(_) => {}
        }

        Err(SuiError::TypeError {
            error: format!("Object type is not a CoinMetadata: {:?}", object),
        })
    }
}
