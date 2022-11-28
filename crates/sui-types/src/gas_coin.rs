// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::MoveStructLayout,
};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt::{Display, Formatter};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    id::UID,
    object::{Data, MoveObject, Object},
    SUI_FRAMEWORK_ADDRESS,
};

pub const GAS_MODULE_NAME: &IdentStr = ident_str!("sui");
pub const GAS_STRUCT_NAME: &IdentStr = ident_str!("SUI");

pub struct GAS {}
impl GAS {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: GAS_STRUCT_NAME.to_owned(),
            module: GAS_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn type_tag() -> TypeTag {
        TypeTag::Struct(Self::type_())
    }

    pub fn is_gas(other: &StructTag) -> bool {
        &Self::type_() == other
    }
}

/// Rust version of the Move sui::coin::Coin<Sui::sui::SUI> type
#[derive(Debug, Serialize, Deserialize)]
pub struct GasCoin(pub Coin);

impl GasCoin {
    pub fn new(id: ObjectID, value: u64) -> Self {
        Self(Coin::new(UID::new(id), value))
    }

    pub fn value(&self) -> u64 {
        self.0.value()
    }

    pub fn type_() -> StructTag {
        Coin::type_(GAS::type_())
    }

    pub fn id(&self) -> &ObjectID {
        self.0.id()
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn to_object(&self, version: SequenceNumber) -> MoveObject {
        MoveObject::new_gas_coin(version, *self.id(), self.value())
    }

    pub fn layout() -> MoveStructLayout {
        Coin::layout(Self::type_())
    }
}

impl TryFrom<&MoveObject> for GasCoin {
    type Error = ExecutionError;

    fn try_from(value: &MoveObject) -> Result<GasCoin, ExecutionError> {
        if value.type_ != GasCoin::type_() {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::InvalidGasObject,
                format!("Gas object type is not a gas coin: {}", value.type_),
            ));
        }
        let gas_coin: GasCoin = bcs::from_bytes(value.contents()).map_err(|err| {
            ExecutionError::new_with_source(
                ExecutionErrorKind::InvalidGasObject,
                format!("Unable to deserialize gas object: {:?}", err),
            )
        })?;
        Ok(gas_coin)
    }
}

impl TryFrom<&Object> for GasCoin {
    type Error = ExecutionError;

    fn try_from(value: &Object) -> Result<GasCoin, ExecutionError> {
        match &value.data {
            Data::Move(obj) => obj.try_into(),
            Data::Package(_) => Err(ExecutionError::new_with_source(
                ExecutionErrorKind::InvalidGasObject,
                format!("Gas object type is not a gas coin: {:?}", value),
            )),
        }
    }
}

impl Display for GasCoin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Coin {{ id: {}, value: {} }}", self.id(), self.value())
    }
}
