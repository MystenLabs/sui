// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::balance::Balance;
use crate::base_types::SuiAddress;
use crate::coin::Coin;
use crate::effects::{
    AccumulatorOperation, AccumulatorValue, TransactionEffects, TransactionEffectsAPI,
};
use crate::full_checkpoint_content::ObjectSet;
use crate::object::Object;
use crate::object::Owner;
use crate::storage::ObjectKey;
use move_core_types::language_storage::TypeTag;

#[derive(Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BalanceChange {
    /// Owner of the balance change
    pub address: SuiAddress,

    /// Type of the Coin
    pub coin_type: TypeTag,

    /// The amount indicate the balance value changes.
    ///
    /// A negative amount means spending coin value and positive means receiving coin value.
    pub amount: i128,
}

impl std::fmt::Debug for BalanceChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BalanceChange")
            .field("address", &self.address)
            .field("coin_type", &self.coin_type.to_canonical_string(true))
            .field("amount", &self.amount)
            .finish()
    }
}

fn coins(objects: &[Object]) -> impl Iterator<Item = (&SuiAddress, TypeTag, u64)> + '_ {
    objects.iter().filter_map(|object| {
        let address = match object.owner() {
            Owner::AddressOwner(sui_address)
            | Owner::ObjectOwner(sui_address)
            | Owner::ConsensusAddressOwner {
                owner: sui_address, ..
            } => sui_address,
            Owner::Shared { .. } | Owner::Immutable => return None,
        };
        let (coin_type, balance) = Coin::extract_balance_if_coin(object).ok().flatten()?;
        Some((address, coin_type, balance))
    })
}

/// Extract balance changes from accumulator events that have Balance<T> types.
/// Returns an iterator of (address, coin_type, signed_amount) tuples.
fn address_balance_changes_from_accumulator_events(
    effects: &TransactionEffects,
) -> impl Iterator<Item = (SuiAddress, TypeTag, i128)> + '_ {
    effects
        .accumulator_events()
        .into_iter()
        .filter_map(|event| {
            let ty = &event.write.address.ty;
            // Only process events with Balance<T> types
            let coin_type = Balance::maybe_get_balance_type_param(ty)?;

            let amount = match &event.write.value {
                AccumulatorValue::Integer(v) => *v as i128,
                // IntegerTuple and EventDigest are not balance-related
                AccumulatorValue::IntegerTuple(_, _) | AccumulatorValue::EventDigest(_) => {
                    return None;
                }
            };

            // Convert operation to signed amount: Split means balance decreased, Merge means increased
            let signed_amount = match event.write.operation {
                AccumulatorOperation::Split => -amount,
                AccumulatorOperation::Merge => amount,
            };

            Some((event.write.address.address, coin_type, signed_amount))
        })
}

pub fn derive_balance_changes(
    effects: &TransactionEffects,
    input_objects: &[Object],
    output_objects: &[Object],
) -> Vec<BalanceChange> {
    // 1. subtract all input coins
    let balances = coins(input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin_type, balance)| {
            *acc.entry((*address, coin_type)).or_default() -= balance as i128;
            acc
        },
    );

    // 2. add all mutated/output coins
    let balances =
        coins(output_objects).fold(balances, |mut acc, (address, coin_type, balance)| {
            *acc.entry((*address, coin_type)).or_default() += balance as i128;
            acc
        });

    // 3. add address balance changes from accumulator events
    let balances = address_balance_changes_from_accumulator_events(effects).fold(
        balances,
        |mut acc, (address, coin_type, signed_amount)| {
            *acc.entry((address, coin_type)).or_default() += signed_amount;
            acc
        },
    );

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address,
                coin_type,
                amount,
            })
        })
        .collect()
}

pub fn derive_balance_changes_2(
    effects: &TransactionEffects,
    objects: &ObjectSet,
) -> Vec<BalanceChange> {
    let input_objects = effects
        .modified_at_versions()
        .into_iter()
        .filter_map(|(object_id, version)| objects.get(&ObjectKey(object_id, version)).cloned())
        .collect::<Vec<_>>();
    let output_objects = effects
        .all_changed_objects()
        .into_iter()
        .filter_map(|(object_ref, _owner, _kind)| objects.get(&object_ref.into()).cloned())
        .collect::<Vec<_>>();

    // 1. subtract all input coins
    let balances = coins(&input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin_type, balance)| {
            *acc.entry((*address, coin_type)).or_default() -= balance as i128;
            acc
        },
    );

    // 2. add all mutated/output coins
    let balances =
        coins(&output_objects).fold(balances, |mut acc, (address, coin_type, balance)| {
            *acc.entry((*address, coin_type)).or_default() += balance as i128;
            acc
        });

    // 3. add address balance changes from accumulator events
    let balances = address_balance_changes_from_accumulator_events(effects).fold(
        balances,
        |mut acc, (address, coin_type, signed_amount)| {
            *acc.entry((address, coin_type)).or_default() += signed_amount;
            acc
        },
    );

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address,
                coin_type,
                amount,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accumulator_root::AccumulatorValue as AccumulatorValueRoot;
    use crate::balance::Balance;
    use crate::base_types::ObjectID;
    use crate::digests::TransactionDigest;
    use crate::effects::{
        AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
        EffectsObjectChange, IDOperation, ObjectIn, ObjectOut, TransactionEffects,
    };
    use crate::execution_status::ExecutionStatus;
    use crate::gas::GasCostSummary;
    use move_core_types::language_storage::TypeTag;

    fn create_effects_with_accumulator_writes(
        writes: Vec<(ObjectID, AccumulatorWriteV1)>,
    ) -> TransactionEffects {
        let changed_objects = writes
            .into_iter()
            .map(|(id, write)| {
                (
                    id,
                    EffectsObjectChange {
                        input_state: ObjectIn::NotExist,
                        output_state: ObjectOut::AccumulatorWriteV1(write),
                        id_operation: IDOperation::None,
                    },
                )
            })
            .collect();

        TransactionEffects::new_from_execution_v2(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![],
            std::collections::BTreeSet::new(),
            TransactionDigest::random(),
            crate::base_types::SequenceNumber::new(),
            changed_objects,
            None,
            None,
            vec![],
        )
    }

    fn sui_balance_type() -> TypeTag {
        Balance::type_tag("0x2::sui::SUI".parse().unwrap())
    }

    fn custom_coin_type() -> TypeTag {
        "0xabc::my_coin::MY_COIN".parse().unwrap()
    }

    fn custom_balance_type() -> TypeTag {
        Balance::type_tag(custom_coin_type())
    }

    fn get_accumulator_obj_id(address: SuiAddress, balance_type: &TypeTag) -> ObjectID {
        *AccumulatorValueRoot::get_field_id(address, balance_type)
            .unwrap()
            .inner()
    }

    #[test]
    fn test_derive_balance_changes_with_no_accumulator_events() {
        let effects = create_effects_with_accumulator_writes(vec![]);
        let result = derive_balance_changes(&effects, &[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_derive_balance_changes_with_split_accumulator_event() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(1000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(
            result[0].coin_type,
            "0x2::sui::SUI".parse::<TypeTag>().unwrap()
        );
        assert_eq!(result[0].amount, -1000);
    }

    #[test]
    fn test_derive_balance_changes_with_merge_accumulator_event() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(500),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(result[0].amount, 500);
    }

    #[test]
    fn test_derive_balance_changes_with_multiple_addresses() {
        let address1 = SuiAddress::random_for_testing_only();
        let address2 = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();

        let obj_id1 = get_accumulator_obj_id(address1, &balance_type);
        let obj_id2 = get_accumulator_obj_id(address2, &balance_type);

        let write1 = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address1, balance_type.clone()),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(1000),
        };
        let write2 = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address2, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(1000),
        };

        let effects =
            create_effects_with_accumulator_writes(vec![(obj_id1, write1), (obj_id2, write2)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert_eq!(result.len(), 2);
        let addr1_change = result.iter().find(|c| c.address == address1).unwrap();
        let addr2_change = result.iter().find(|c| c.address == address2).unwrap();
        assert_eq!(addr1_change.amount, -1000);
        assert_eq!(addr2_change.amount, 1000);
    }

    #[test]
    fn test_derive_balance_changes_with_custom_coin_type() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = custom_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(2000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(result[0].coin_type, custom_coin_type());
        assert_eq!(result[0].amount, -2000);
    }

    #[test]
    fn test_derive_balance_changes_ignores_non_balance_types() {
        let address = SuiAddress::random_for_testing_only();
        // Use a non-Balance type - use a random ObjectID since we can't derive it
        let non_balance_type: TypeTag = "0x2::accumulator_settlement::EventStreamHead"
            .parse()
            .unwrap();
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, non_balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(1000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(ObjectID::random(), write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert!(result.is_empty());
    }

    #[test]
    fn test_derive_balance_changes_ignores_event_digest_values() {
        use crate::digests::Digest;
        use nonempty::nonempty;

        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::EventDigest(nonempty![(0, Digest::random())]),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        assert!(result.is_empty());
    }

    #[test]
    fn test_derive_balance_changes_accumulator_zero_amount_filtered() {
        // Test that a zero amount accumulator event results in no balance change
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);

        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(0),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[], &[]);

        // Zero amount should be filtered out
        assert!(result.is_empty());
    }

    #[test]
    fn test_derive_balance_changes_2_with_accumulator_events() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(1000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let objects = crate::full_checkpoint_content::ObjectSet::default();
        let result = derive_balance_changes_2(&effects, &objects);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(
            result[0].coin_type,
            "0x2::sui::SUI".parse::<TypeTag>().unwrap()
        );
        assert_eq!(result[0].amount, -1000);
    }

    // Tests combining coin objects with accumulator events

    fn create_gas_coin_object(owner: SuiAddress, value: u64) -> Object {
        use crate::base_types::SequenceNumber;
        use crate::object::MoveObject;

        let obj_id = ObjectID::random();
        let move_obj = MoveObject::new_gas_coin(SequenceNumber::new(), obj_id, value);
        Object::new_move(
            move_obj,
            Owner::AddressOwner(owner),
            TransactionDigest::random(),
        )
    }

    fn create_custom_coin_object(owner: SuiAddress, coin_type: TypeTag, value: u64) -> Object {
        use crate::base_types::SequenceNumber;
        use crate::object::MoveObject;

        let obj_id = ObjectID::random();
        let move_obj = MoveObject::new_coin(coin_type, SequenceNumber::new(), obj_id, value);
        Object::new_move(
            move_obj,
            Owner::AddressOwner(owner),
            TransactionDigest::random(),
        )
    }

    #[test]
    fn test_derive_balance_changes_with_coin_objects_only() {
        let address = SuiAddress::random_for_testing_only();

        // Create input coin with 5000 SUI
        let input_coin = create_gas_coin_object(address, 5000);
        // Create output coin with 3000 SUI (spent 2000)
        let output_coin = create_gas_coin_object(address, 3000);

        let effects = create_effects_with_accumulator_writes(vec![]);

        let result = derive_balance_changes(&effects, &[input_coin], &[output_coin]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(result[0].amount, -2000); // 3000 - 5000 = -2000
    }

    #[test]
    fn test_derive_balance_changes_coin_transfer_between_addresses() {
        let sender = SuiAddress::random_for_testing_only();
        let receiver = SuiAddress::random_for_testing_only();

        // Sender has 10000 SUI initially
        let input_coin = create_gas_coin_object(sender, 10000);
        // After transfer: sender keeps 7000, receiver gets 3000
        let output_coin_sender = create_gas_coin_object(sender, 7000);
        let output_coin_receiver = create_gas_coin_object(receiver, 3000);

        let effects = create_effects_with_accumulator_writes(vec![]);

        let result = derive_balance_changes(
            &effects,
            &[input_coin],
            &[output_coin_sender, output_coin_receiver],
        );

        assert_eq!(result.len(), 2);
        let sender_change = result.iter().find(|c| c.address == sender).unwrap();
        let receiver_change = result.iter().find(|c| c.address == receiver).unwrap();
        assert_eq!(sender_change.amount, -3000); // 7000 - 10000 = -3000
        assert_eq!(receiver_change.amount, 3000); // 3000 - 0 = 3000
    }

    #[test]
    fn test_derive_balance_changes_combines_coins_and_accumulator_events() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);

        // Coin: spent 2000 SUI (5000 -> 3000)
        let input_coin = create_gas_coin_object(address, 5000);
        let output_coin = create_gas_coin_object(address, 3000);

        // Accumulator: received 500 SUI via address balance
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(500),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[input_coin], &[output_coin]);

        // Net change: -2000 (coins) + 500 (accumulator merge) = -1500
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, address);
        assert_eq!(result[0].amount, -1500);
    }

    #[test]
    fn test_derive_balance_changes_coins_and_accumulator_different_addresses() {
        let coin_owner = SuiAddress::random_for_testing_only();
        let accumulator_owner = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(accumulator_owner, &balance_type);

        // Coin owner spends 1000 SUI
        let input_coin = create_gas_coin_object(coin_owner, 5000);
        let output_coin = create_gas_coin_object(coin_owner, 4000);

        // Accumulator owner receives 2000 SUI
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(accumulator_owner, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(2000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[input_coin], &[output_coin]);

        assert_eq!(result.len(), 2);
        let coin_change = result.iter().find(|c| c.address == coin_owner).unwrap();
        let acc_change = result
            .iter()
            .find(|c| c.address == accumulator_owner)
            .unwrap();
        assert_eq!(coin_change.amount, -1000);
        assert_eq!(acc_change.amount, 2000);
    }

    #[test]
    fn test_derive_balance_changes_coins_and_accumulator_net_to_zero() {
        let address = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let obj_id = get_accumulator_obj_id(address, &balance_type);

        // Coin: spent 1000 SUI (5000 -> 4000)
        let input_coin = create_gas_coin_object(address, 5000);
        let output_coin = create_gas_coin_object(address, 4000);

        // Accumulator: received exactly 1000 SUI - perfectly offsets coin spend
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(1000),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[input_coin], &[output_coin]);

        // Net change: -1000 (coins) + 1000 (accumulator) = 0, should be filtered out
        assert!(result.is_empty());
    }

    #[test]
    fn test_derive_balance_changes_different_coin_types() {
        let address = SuiAddress::random_for_testing_only();
        let custom_type = custom_coin_type();
        let custom_balance = custom_balance_type();
        let obj_id = get_accumulator_obj_id(address, &custom_balance);

        // SUI coin: spent 1000
        let sui_input = create_gas_coin_object(address, 5000);
        let sui_output = create_gas_coin_object(address, 4000);

        // Custom coin: received 500 via coins
        let custom_output = create_custom_coin_object(address, custom_type.clone(), 500);

        // Custom coin: also received 300 via accumulator
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, custom_balance),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(300),
        };
        let effects = create_effects_with_accumulator_writes(vec![(obj_id, write)]);

        let result = derive_balance_changes(&effects, &[sui_input], &[sui_output, custom_output]);

        assert_eq!(result.len(), 2);

        let sui_change = result
            .iter()
            .find(|c| c.coin_type == "0x2::sui::SUI".parse::<TypeTag>().unwrap())
            .unwrap();
        let custom_change = result.iter().find(|c| c.coin_type == custom_type).unwrap();

        assert_eq!(sui_change.amount, -1000);
        assert_eq!(custom_change.amount, 800); // 500 (coin) + 300 (accumulator)
    }

    #[test]
    fn test_derive_balance_changes_accumulator_split_with_coins() {
        let sender = SuiAddress::random_for_testing_only();
        let receiver = SuiAddress::random_for_testing_only();
        let balance_type = sui_balance_type();
        let sender_obj_id = get_accumulator_obj_id(sender, &balance_type);
        let receiver_obj_id = get_accumulator_obj_id(receiver, &balance_type.clone());

        // Sender sends coins: 5000 -> 4000 (sends 1000)
        let input_coin = create_gas_coin_object(sender, 5000);
        let output_coin = create_gas_coin_object(sender, 4000);

        // Sender also sends 500 via address balance (Merge)
        let sender_write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(sender, balance_type.clone()),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(500),
        };
        // Receiver receives 500 via address balance (Split)
        let receiver_write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(receiver, balance_type),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(500),
        };
        let effects = create_effects_with_accumulator_writes(vec![
            (sender_obj_id, sender_write),
            (receiver_obj_id, receiver_write),
        ]);

        let result = derive_balance_changes(&effects, &[input_coin], &[output_coin]);

        assert_eq!(result.len(), 2);
        let sender_change = result.iter().find(|c| c.address == sender).unwrap();
        let receiver_change = result.iter().find(|c| c.address == receiver).unwrap();

        // Sender: -1000 (coins) + -500 (accumulator merge) = -1500
        assert_eq!(sender_change.amount, -1500);
        // Receiver: +500 (accumulator split)
        assert_eq!(receiver_change.amount, 500);
    }
}
