// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use tracing::{debug, trace};

use sui_json_rpc_types::{GetRawObjectDataResponse, SuiData, SuiEvent, SuiRawObject};
use sui_sdk::SuiClient;
use sui_types::gas_coin::GasCoin;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    object::Owner,
};

/// A util struct that helps verify Sui Object.
/// Use builder style to construct the conditions.
/// When optionals fields are not set, related checks are omitted.
/// Consuming functions such as `check` perform the check and panics if
/// verification results are unexpected. `check_into_object` and
/// `check_into_gas_coin` expect to get a `SuiRawObject` and `GasCoin`
/// respectfully.
/// ```
#[derive(Debug)]
pub struct ObjectChecker {
    object_id: ObjectID,
    owner: Option<Owner>,
    is_deleted: bool,
    is_sui_coin: Option<bool>,
}

impl ObjectChecker {
    pub fn new(object_id: ObjectID) -> ObjectChecker {
        Self {
            object_id,
            owner: None,
            is_deleted: false, // default to exist
            is_sui_coin: None,
        }
    }

    pub fn owner(mut self, owner: Owner) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn deleted(mut self) -> Self {
        self.is_deleted = true;
        self
    }

    pub fn is_sui_coin(mut self, is_sui_coin: bool) -> Self {
        self.is_sui_coin = Some(is_sui_coin);
        self
    }

    pub async fn check_into_gas_coin(self, client: &SuiClient) -> GasCoin {
        if self.is_sui_coin == Some(false) {
            panic!("'check_into_gas_coin' shouldn't be called with 'is_sui_coin' set as false");
        }
        self.is_sui_coin(true)
            .check(client)
            .await
            .unwrap()
            .into_gas_coin()
    }

    pub async fn check_into_object(self, client: &SuiClient) -> SuiRawObject {
        self.check(client).await.unwrap().into_object()
    }

    pub async fn check(self, client: &SuiClient) -> Result<CheckerResultObject, anyhow::Error> {
        debug!(?self);

        let object_id = self.object_id;
        let object_info = client
            .read_api()
            .get_object(object_id)
            .await
            .or_else(|err| bail!("Failed to get object info (id: {}), err: {err}", object_id))?;

        trace!("getting object {object_id}, info :: {object_info:?}");

        match object_info {
            GetRawObjectDataResponse::NotExists(_) => {
                panic!(
                    "Node can't find gas object {} with client {:?}",
                    object_id,
                    client.read_api()
                )
            }
            GetRawObjectDataResponse::Deleted(_) => {
                if !self.is_deleted {
                    panic!("Gas object {} was deleted", object_id);
                }
                Ok(CheckerResultObject::new(None, None))
            }
            GetRawObjectDataResponse::Exists(object) => {
                if self.is_deleted {
                    panic!("Expect Gas object {} deleted, but it is not", object_id);
                }
                if let Some(owner) = self.owner {
                    assert_eq!(
                        object.owner, owner,
                        "Gas coin {} does not belong to {}, but {}",
                        object_id, owner, object.owner
                    );
                }
                if self.is_sui_coin == Some(true) {
                    let move_obj = object
                        .data
                        .try_as_move()
                        .unwrap_or_else(|| panic!("Object {} is not a move object", object_id));

                    let gas_coin = move_obj.deserialize()?;
                    return Ok(CheckerResultObject::new(Some(gas_coin), Some(object)));
                }
                Ok(CheckerResultObject::new(None, Some(object)))
            }
        }
    }
}

pub struct CheckerResultObject {
    gas_coin: Option<GasCoin>,
    object: Option<SuiRawObject>,
}

impl CheckerResultObject {
    pub fn new(gas_coin: Option<GasCoin>, object: Option<SuiRawObject>) -> Self {
        Self { gas_coin, object }
    }
    pub fn into_gas_coin(self) -> GasCoin {
        self.gas_coin.unwrap()
    }
    pub fn into_object(self) -> SuiRawObject {
        self.object.unwrap()
    }
}

#[macro_export]
macro_rules! assert_eq_if_present {
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match (&$left, &$right) {
            (Some(left_val), right_val) => {
                 if !(&left_val == right_val) {
                    panic!("{} does not match, left: {:?}, right: {:?}", $($arg)+, left_val, right_val);
                }
            }
            _ => ()
        }
    };
}

#[derive(Default, Debug)]
pub struct CoinBalanceChangeEventChecker {
    package_id: Option<ObjectID>,
    transaction_module: Option<String>,
    sender: Option<SuiAddress>,
    owner: Option<Owner>,
    coin_type: Option<String>,
    coin_object_id: Option<ObjectID>,
    version: Option<SequenceNumber>,
    amount: Option<i128>,
}

impl CoinBalanceChangeEventChecker {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn package_id(mut self, package_id: ObjectID) -> Self {
        self.package_id = Some(package_id);
        self
    }
    pub fn transaction_module(mut self, transaction_module: String) -> Self {
        self.transaction_module = Some(transaction_module);
        self
    }
    pub fn sender(mut self, sender: SuiAddress) -> Self {
        self.sender = Some(sender);
        self
    }
    pub fn owner(mut self, owner: Owner) -> Self {
        self.owner = Some(owner);
        self
    }
    pub fn coin_type(mut self, coin_type: &str) -> Self {
        self.coin_type = Some(coin_type.to_string());
        self
    }
    pub fn coin_object_id(mut self, coin_object_id: ObjectID) -> Self {
        self.coin_object_id = Some(coin_object_id);
        self
    }
    pub fn version(mut self, version: SequenceNumber) -> Self {
        self.version = Some(version);
        self
    }

    pub fn amount(mut self, amount: i128) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn check(self, event: &SuiEvent) {
        if let SuiEvent::CoinBalanceChange {
            package_id,
            transaction_module,
            sender,
            owner,
            coin_type,
            coin_object_id,
            version,
            amount,
        } = event
        {
            assert_eq_if_present!(self.package_id, package_id, "package_id");
            assert_eq_if_present!(
                self.transaction_module,
                transaction_module,
                "transaction_module"
            );
            assert_eq_if_present!(self.sender, sender, "sender");
            assert_eq_if_present!(self.owner, owner, "owner");
            assert_eq_if_present!(self.coin_type, coin_type, "coin_type");
            assert_eq_if_present!(self.coin_object_id, coin_object_id, "coin_object_id");
            assert_eq_if_present!(self.version, version, "version");
            assert_eq_if_present!(self.amount, amount, "version");
        } else {
            panic!("event {:?} is not TransferObject Event", event);
        }
    }
}
