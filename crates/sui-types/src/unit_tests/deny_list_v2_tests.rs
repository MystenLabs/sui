// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::base_types::{ObjectID, SequenceNumber};
use crate::config::SettingData;
use crate::digests::TransactionDigest;
use crate::dynamic_field::DynamicFieldKey;
use crate::error::{SuiError, SuiErrorKind};
use crate::in_memory_storage::InMemoryStorage;
use crate::object::{Object, Owner};
use crate::parse_sui_type_tag;
use crate::transaction::InputObjects;
use std::cell::RefCell;

fn coin(module: &str) -> TypeTag {
    parse_sui_type_tag(&format!("0x123::{module}::COIN")).unwrap()
}

fn name(coin_type: &TypeTag) -> String {
    coin_type.to_canonical_string(false)
}

fn alice() -> SuiAddress {
    SuiAddress::from(ObjectID::from_hex_literal("0xa11ce").unwrap())
}

fn bob() -> SuiAddress {
    SuiAddress::from(ObjectID::from_hex_literal("0xb0b").unwrap())
}

fn field<K, V>(parent: ObjectID, key: K, value: V) -> Object
where
    K: MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug,
    V: MoveTypeTagTrait + Serialize + DeserializeOwned,
{
    let field = DynamicFieldKey(parent, key, K::get_type_tag())
        .into_field(value)
        .unwrap()
        .into_move_object_unsafe_for_testing(SequenceNumber::from_u64(1))
        .unwrap();
    Object::new_move(
        field,
        Owner::ObjectOwner(parent.into()),
        TransactionDigest::genesis_marker(),
    )
}

fn denied() -> Setting<bool> {
    Setting {
        data: Some(SettingData {
            newer_value_epoch: 0,
            newer_value: Some(true),
            older_value_opt: None,
        }),
    }
}

/// Gives `coin_type` a legacy deny list denying `denied_addresses`, and pausing it if `paused`.
fn deny(
    objects: &mut Vec<Object>,
    coin_type: &TypeTag,
    paused: bool,
    denied_addresses: &[SuiAddress],
) {
    let config_id = ObjectID::random();
    let config_key = DOFWrapper {
        name: ConfigKey {
            per_type_index: DENY_LIST_COIN_TYPE_INDEX,
            per_type_key: name(coin_type).into_bytes(),
        },
    };
    objects.push(field(SUI_DENY_LIST_OBJECT_ID, config_key, config_id));
    if paused {
        objects.push(field(config_id, GlobalPauseKey::new(), denied()));
    }
    for address in denied_addresses {
        objects.push(field(config_id, AddressKey(*address), denied()));
    }
}

/// Records every coin type `is_migrated` is asked about, answering true for `migrated`.
struct Migrations {
    migrated: Vec<String>,
    asked: RefCell<Vec<String>>,
}

impl Migrations {
    fn new(migrated: &[&TypeTag]) -> Self {
        Self {
            migrated: migrated.iter().map(|t| name(t)).collect(),
            asked: RefCell::new(vec![]),
        }
    }

    fn is_migrated(&self, coin_type: &str) -> bool {
        self.asked.borrow_mut().push(coin_type.to_owned());
        self.migrated.iter().any(|m| m == coin_type)
    }

    fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

fn sign(
    store: &InMemoryStorage,
    withdrawals: &[(&TypeTag, SuiAddress)],
    migrations: &Migrations,
) -> SuiResult {
    check_coin_deny_list_v2_during_signing(
        bob(),
        &CheckedInputObjects::new_for_replay(InputObjects::new(vec![])),
        &ReceivingObjects { objects: vec![] },
        withdrawals
            .iter()
            .map(|(coin_type, funder)| (*funder, name(coin_type)))
            .collect(),
        store,
        |coin_type| Ok(migrations.is_migrated(coin_type)),
    )
}

fn user_input_error(result: SuiResult) -> Option<UserInputError> {
    match result.err().map(SuiError::into_inner) {
        None => None,
        Some(SuiErrorKind::UserInputError { error }) => Some(error),
        Some(other) => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn signing_skips_migrated_types() {
    let (a, b, c) = (coin("a"), coin("b"), coin("c"));
    let mut objects = vec![];
    deny(&mut objects, &a, true, &[alice()]);
    deny(&mut objects, &b, false, &[alice()]);
    let store = InMemoryStorage::new(objects);
    let withdrawals = [(&a, alice()), (&b, alice()), (&c, alice())];

    let migrations = Migrations::new(&[&a]);
    assert_eq!(
        user_input_error(sign(&store, &withdrawals, &migrations)),
        Some(UserInputError::AddressDeniedForCoin {
            address: alice(),
            coin_type: name(&b),
        })
    );
    assert_eq!(migrations.asked(), vec![name(&a), name(&b)]);

    let migrations = Migrations::new(&[&a, &b]);
    assert_eq!(
        user_input_error(sign(&store, &withdrawals, &migrations)),
        None
    );
    // Every type is asked before its deny list is read, including one without a deny list.
    assert_eq!(migrations.asked(), vec![name(&a), name(&b), name(&c)]);

    let migrations = Migrations::new(&[]);
    assert_eq!(
        user_input_error(sign(&store, &withdrawals, &migrations)),
        Some(UserInputError::CoinTypeGlobalPause {
            coin_type: name(&a)
        })
    );
}
