// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::ExecutionErrorKind;
use crate::error::SuiError;
use crate::{
    balance::{Balance, Supply},
    error::ExecutionError,
    object::{Data, Object},
};
use crate::{base_types::ObjectID, id::UID, SUI_FRAMEWORK_ADDRESS};
use move_core_types::{
    annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub fn new(id: ObjectID, value: u64) -> Self {
        Self {
            id: UID::new(id),
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
    pub fn extract_balance_if_coin(object: &Object) -> Result<Option<(TypeTag, u64)>, bcs::Error> {
        let Data::Move(obj) = &object.data else {
            return Ok(None);
        };

        let Some(type_) = obj.type_().coin_type_maybe() else {
            return Ok(None);
        };

        let coin = Self::from_bcs_bytes(obj.contents())?;
        Ok(Some((type_, coin.value())))
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
        MoveStructLayout {
            type_: Self::type_(type_param.clone()),
            fields: vec![
                MoveFieldLayout::new(
                    ident_str!("id").to_owned(),
                    MoveTypeLayout::Struct(Box::new(UID::layout())),
                ),
                MoveFieldLayout::new(
                    ident_str!("balance").to_owned(),
                    MoveTypeLayout::Struct(Box::new(Balance::layout(type_param))),
                ),
            ],
        }
    }

    /// Add balance to this coin, erroring if the new total balance exceeds the maximum
    pub fn add(&mut self, balance: Balance) -> Result<(), ExecutionError> {
        let Some(new_value) = self.value().checked_add(balance.value()) else {
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
    pub fn split(&mut self, amount: u64, new_coin_id: ObjectID) -> Result<Coin, ExecutionError> {
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
    pub fn is_treasury_type(other: &StructTag) -> bool {
        other.address == SUI_FRAMEWORK_ADDRESS
            && other.module.as_ident_str() == COIN_MODULE_NAME
            && other.name.as_ident_str() == COIN_TREASURE_CAP_NAME
    }

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

    /// Checks if the provided type is `TreasuryCap<T>`, returning the type T if so.
    pub fn is_treasury_with_coin_type(other: &StructTag) -> Option<&StructTag> {
        if Self::is_treasury_type(other) && other.type_params.len() == 1 {
            match other.type_params.first() {
                Some(TypeTag::Struct(coin_type)) => Some(coin_type),
                _ => None,
            }
        } else {
            None
        }
    }
}

impl TryFrom<Object> for TreasuryCap {
    type Error = SuiError;
    fn try_from(object: Object) -> Result<Self, Self::Error> {
        match &object.data {
            Data::Move(o) => {
                if o.type_().is_treasury_cap() {
                    return TreasuryCap::from_bcs_bytes(o.contents());
                }
            }
            Data::Package(_) => {}
        }

        Err(SuiError::TypeError {
            error: format!("Object type is not a TreasuryCap: {:?}", object),
        })
    }
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

    /// Checks if the provided type is `CoinMetadata<T>`, returning the type T if so.
    pub fn is_coin_metadata_with_coin_type(other: &StructTag) -> Option<&StructTag> {
        if Self::is_coin_metadata(other) && other.type_params.len() == 1 {
            match other.type_params.first() {
                Some(TypeTag::Struct(coin_type)) => Some(coin_type),
                _ => None,
            }
        } else {
            None
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
