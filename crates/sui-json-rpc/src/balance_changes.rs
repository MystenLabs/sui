// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use sui_types::balance_change::derive_balance_changes;
use tokio::sync::RwLock;

use sui_json_rpc_types::BalanceChange;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::digests::ObjectDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::{Object, Owner};
use sui_types::storage::WriteKind;
use sui_types::transaction::InputObjectKind;
use tracing::instrument;

#[instrument(skip_all, fields(transaction_digest = %effects.transaction_digest()))]
pub async fn get_balance_changes_from_effect<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    effects: &TransactionEffects,
    input_objs: Vec<InputObjectKind>,
    mocked_coin: Option<ObjectID>,
) -> Result<Vec<BalanceChange>, E> {
    let all_mutated = effects
        .all_changed_objects()
        .into_iter()
        .filter_map(|((id, version, digest), _, _)| {
            if matches!(mocked_coin, Some(coin) if id == coin) {
                return None;
            }
            Some((id, version, Some(digest)))
        })
        .collect::<Vec<_>>();

    let input_objs_to_digest = input_objs
        .iter()
        .filter_map(|k| match k {
            InputObjectKind::ImmOrOwnedMoveObject(o) => Some((o.0, o.2)),
            InputObjectKind::MovePackage(_) | InputObjectKind::SharedMoveObject { .. } => None,
        })
        .collect::<HashMap<ObjectID, ObjectDigest>>();
    let unwrapped_then_deleted = effects
        .unwrapped_then_deleted()
        .iter()
        .map(|e| e.0)
        .collect::<HashSet<_>>();

    let modified_at_version = effects
        .modified_at_versions()
        .into_iter()
        .filter_map(|(id, version)| {
            if matches!(mocked_coin, Some(coin) if id == coin) {
                return None;
            }
            // We won't be able to get dynamic object from object provider today
            if unwrapped_then_deleted.contains(&id) {
                return None;
            }
            Some((id, version, input_objs_to_digest.get(&id).cloned()))
        })
        .collect::<Vec<_>>();
    let input_coins = fetch_coins(object_provider, &modified_at_version).await?;
    let mutated_coins = fetch_coins(object_provider, &all_mutated).await?;
    Ok(
        derive_balance_changes(effects, &input_coins, &mutated_coins)
            .into_iter()
            .map(|change| BalanceChange {
                owner: Owner::AddressOwner(change.address),
                coin_type: change.coin_type,
                amount: change.amount,
            })
            .collect(),
    )
}

#[instrument(skip_all)]
async fn fetch_coins<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    objects: &[(ObjectID, SequenceNumber, Option<ObjectDigest>)],
) -> Result<Vec<Object>, E> {
    let mut coins = vec![];
    for (id, version, digest_opt) in objects {
        // TODO: use multi get object
        let o = object_provider.get_object(id, version).await?;
        if let Some(type_) = o.type_()
            && type_.is_coin()
        {
            if let Some(digest) = digest_opt {
                // TODO: can we return Err here instead?
                assert_eq!(
                    *digest,
                    o.digest(),
                    "Object digest mismatch--got bad data from object_provider?"
                )
            }
            coins.push(o);
        }
    }
    Ok(coins)
}

#[async_trait]
pub trait ObjectProvider {
    type Error;
    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error>;
    async fn find_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error>;
}

pub struct ObjectProviderCache<P> {
    object_cache: RwLock<BTreeMap<(ObjectID, SequenceNumber), Object>>,
    last_version_cache: RwLock<BTreeMap<(ObjectID, SequenceNumber), SequenceNumber>>,
    provider: P,
}

impl<P> ObjectProviderCache<P> {
    pub fn new(provider: P) -> Self {
        Self {
            object_cache: Default::default(),
            last_version_cache: Default::default(),
            provider,
        }
    }

    pub fn insert_objects_into_cache(&mut self, objects: Vec<Object>) {
        let object_cache = self.object_cache.get_mut();
        let last_version_cache = self.last_version_cache.get_mut();

        for object in objects {
            let object_id = object.id();
            let version = object.version();

            let key = (object_id, version);
            object_cache.insert(key, object.clone());

            match last_version_cache.get_mut(&key) {
                Some(existing_seq_number) => {
                    if version > *existing_seq_number {
                        *existing_seq_number = version
                    }
                }
                None => {
                    last_version_cache.insert(key, version);
                }
            }
        }
    }

    pub fn new_with_cache(
        provider: P,
        written_objects: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
    ) -> Self {
        let mut object_cache = BTreeMap::new();
        let mut last_version_cache = BTreeMap::new();

        for (object_id, (object_ref, object, _)) in written_objects {
            let key = (object_id, object_ref.1);
            object_cache.insert(key, object.clone());

            match last_version_cache.get_mut(&key) {
                Some(existing_seq_number) => {
                    if object_ref.1 > *existing_seq_number {
                        *existing_seq_number = object_ref.1
                    }
                }
                None => {
                    last_version_cache.insert(key, object_ref.1);
                }
            }
        }

        Self {
            object_cache: RwLock::new(object_cache),
            last_version_cache: RwLock::new(last_version_cache),
            provider,
        }
    }
}

#[async_trait]
impl<P, E> ObjectProvider for ObjectProviderCache<P>
where
    P: ObjectProvider<Error = E> + Sync + Send,
    E: Sync + Send,
{
    type Error = P::Error;

    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error> {
        if let Some(o) = self.object_cache.read().await.get(&(*id, *version)) {
            return Ok(o.clone());
        }
        let o = self.provider.get_object(id, version).await?;
        self.object_cache
            .write()
            .await
            .insert((*id, *version), o.clone());
        Ok(o)
    }

    async fn find_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error> {
        if let Some(version) = self.last_version_cache.read().await.get(&(*id, *version)) {
            return Ok(self.get_object(id, version).await.ok());
        }
        if let Some(o) = self
            .provider
            .find_object_lt_or_eq_version(id, version)
            .await?
        {
            self.object_cache
                .write()
                .await
                .insert((*id, o.version()), o.clone());
            self.last_version_cache
                .write()
                .await
                .insert((*id, *version), o.version());
            Ok(Some(o))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use sui_types::accumulator_root::AccumulatorValue as AccumulatorValueRoot;
    use sui_types::balance::Balance;
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::{
        AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
        EffectsObjectChange,
    };
    use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
    use sui_types::gas::GasCostSummary;
    use sui_types::gas_coin::GAS;
    use sui_types::object::MoveObject;

    struct MockObjectProvider {
        objects: HashMap<(ObjectID, SequenceNumber), Object>,
    }

    impl MockObjectProvider {
        fn new() -> Self {
            Self {
                objects: HashMap::new(),
            }
        }

        fn insert(&mut self, obj: Object) {
            self.objects.insert((obj.id(), obj.version()), obj);
        }
    }

    #[async_trait]
    impl ObjectProvider for MockObjectProvider {
        type Error = anyhow::Error;
        async fn get_object(
            &self,
            id: &ObjectID,
            version: &SequenceNumber,
        ) -> Result<Object, Self::Error> {
            self.objects
                .get(&(*id, *version))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Object not found: {} v{}", id, version))
        }
        async fn find_object_lt_or_eq_version(
            &self,
            id: &ObjectID,
            version: &SequenceNumber,
        ) -> Result<Option<Object>, Self::Error> {
            let result = self
                .objects
                .iter()
                .filter(|((oid, v), _)| oid == id && v <= version)
                .max_by_key(|((_, v), _)| *v)
                .map(|(_, obj)| obj.clone());
            Ok(result)
        }
    }

    fn create_failed_effects_with_gas_coin(
        gas_owner: SuiAddress,
        gas_deduction: u64,
    ) -> (TransactionEffects, MockObjectProvider) {
        let gas_id = ObjectID::random();
        let old_version = SequenceNumber::from_u64(1);
        let lamport_version = SequenceNumber::from_u64(2);
        let initial_value = 1_000_000 + gas_deduction;
        let final_value = 1_000_000u64;

        let input_obj = Object::new_move(
            MoveObject::new_gas_coin(old_version, gas_id, initial_value),
            Owner::AddressOwner(gas_owner),
            TransactionDigest::random(),
        );
        let output_obj = Object::new_move(
            MoveObject::new_gas_coin(lamport_version, gas_id, final_value),
            Owner::AddressOwner(gas_owner),
            TransactionDigest::random(),
        );

        let mut changed_objects = BTreeMap::new();
        changed_objects.insert(
            gas_id,
            EffectsObjectChange::new(
                Some((
                    (old_version, input_obj.digest()),
                    Owner::AddressOwner(gas_owner),
                )),
                Some(&output_obj),
                false,
                false,
            ),
        );

        let effects = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::new_failure(ExecutionFailureStatus::InsufficientGas, None),
            0,
            GasCostSummary::new(gas_deduction, 0, 0, 0),
            vec![],
            std::collections::BTreeSet::new(),
            TransactionDigest::random(),
            lamport_version,
            changed_objects,
            Some(gas_id),
            None,
            vec![],
        );

        let mut provider = MockObjectProvider::new();
        provider.insert(input_obj);
        provider.insert(output_obj);

        (effects, provider)
    }

    fn sui_balance_type() -> move_core_types::language_storage::TypeTag {
        Balance::type_tag("0x2::sui::SUI".parse().unwrap())
    }

    fn create_accumulator_write(
        address: SuiAddress,
        amount: u64,
    ) -> (ObjectID, EffectsObjectChange) {
        let balance_type = sui_balance_type();
        let obj_id = *AccumulatorValueRoot::get_field_id(address, &balance_type)
            .unwrap()
            .inner();
        let write = AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, balance_type),
            operation: AccumulatorOperation::Split,
            value: AccumulatorValue::Integer(amount),
        };
        (
            obj_id,
            EffectsObjectChange::new_from_accumulator_write(write),
        )
    }

    fn create_failed_effects_with_accumulator(
        address: SuiAddress,
        amount: u64,
    ) -> TransactionEffects {
        let (obj_id, change) = create_accumulator_write(address, amount);
        let mut changed_objects = BTreeMap::new();
        changed_objects.insert(obj_id, change);

        TransactionEffects::new_from_execution_v2(
            ExecutionStatus::new_failure(ExecutionFailureStatus::InsufficientGas, None),
            0,
            GasCostSummary::new(amount, 0, 0, 0),
            vec![],
            std::collections::BTreeSet::new(),
            TransactionDigest::random(),
            SequenceNumber::new(),
            changed_objects,
            None,
            None,
            vec![],
        )
    }

    fn create_failed_effects_with_gas_coin_and_accumulator(
        gas_owner: SuiAddress,
        gas_deduction: u64,
        acc_address: SuiAddress,
        acc_amount: u64,
    ) -> (TransactionEffects, MockObjectProvider) {
        let gas_id = ObjectID::random();
        let old_version = SequenceNumber::from_u64(1);
        let lamport_version = SequenceNumber::from_u64(2);
        let initial_value = 1_000_000 + gas_deduction;
        let final_value = 1_000_000u64;

        let input_obj = Object::new_move(
            MoveObject::new_gas_coin(old_version, gas_id, initial_value),
            Owner::AddressOwner(gas_owner),
            TransactionDigest::random(),
        );
        let output_obj = Object::new_move(
            MoveObject::new_gas_coin(lamport_version, gas_id, final_value),
            Owner::AddressOwner(gas_owner),
            TransactionDigest::random(),
        );

        let mut changed_objects = BTreeMap::new();
        changed_objects.insert(
            gas_id,
            EffectsObjectChange::new(
                Some((
                    (old_version, input_obj.digest()),
                    Owner::AddressOwner(gas_owner),
                )),
                Some(&output_obj),
                false,
                false,
            ),
        );

        let (acc_obj_id, acc_change) = create_accumulator_write(acc_address, acc_amount);
        changed_objects.insert(acc_obj_id, acc_change);

        let effects = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::new_failure(ExecutionFailureStatus::InsufficientGas, None),
            0,
            GasCostSummary::new(gas_deduction, 0, 0, 0),
            vec![],
            std::collections::BTreeSet::new(),
            TransactionDigest::random(),
            lamport_version,
            changed_objects,
            Some(gas_id),
            None,
            vec![],
        );

        let mut provider = MockObjectProvider::new();
        provider.insert(input_obj);
        provider.insert(output_obj);

        (effects, provider)
    }

    #[tokio::test]
    async fn test_failed_txn_coin_gas_balance_change() {
        let gas_owner = SuiAddress::random_for_testing_only();
        let (effects, provider) = create_failed_effects_with_gas_coin(gas_owner, 1000);

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, Owner::AddressOwner(gas_owner));
        assert_eq!(result[0].coin_type, GAS::type_tag());
        assert_eq!(result[0].amount, -1000);
    }

    #[tokio::test]
    async fn test_failed_txn_address_balance_gas_balance_change() {
        let address = SuiAddress::random_for_testing_only();
        let effects = create_failed_effects_with_accumulator(address, 500);
        let provider = MockObjectProvider::new();

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, Owner::AddressOwner(address));
        assert_eq!(result[0].amount, -500);
    }

    #[tokio::test]
    async fn test_failed_txn_zero_coin_gas_returns_empty() {
        let gas_owner = SuiAddress::random_for_testing_only();
        let (effects, provider) = create_failed_effects_with_gas_coin(gas_owner, 0);

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_failed_txn_zero_gas_no_objects_returns_empty() {
        let effects = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::new_failure(ExecutionFailureStatus::InsufficientGas, None),
            0,
            GasCostSummary::new(0, 0, 0, 0),
            vec![],
            std::collections::BTreeSet::new(),
            TransactionDigest::random(),
            SequenceNumber::new(),
            BTreeMap::new(),
            None,
            None,
            vec![],
        );
        let provider = MockObjectProvider::new();

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_failed_txn_sponsored_address_balance_gas() {
        let sponsor = SuiAddress::random_for_testing_only();
        let effects = create_failed_effects_with_accumulator(sponsor, 750);
        let provider = MockObjectProvider::new();

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, Owner::AddressOwner(sponsor));
        assert_eq!(result[0].amount, -750);
    }

    #[tokio::test]
    async fn test_failed_txn_coin_gas_and_accumulator_different_addresses() {
        let gas_owner = SuiAddress::random_for_testing_only();
        let acc_address = SuiAddress::random_for_testing_only();
        let (effects, provider) =
            create_failed_effects_with_gas_coin_and_accumulator(gas_owner, 1000, acc_address, 200);

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
        let gas_change = result
            .iter()
            .find(|c| c.owner == Owner::AddressOwner(gas_owner))
            .unwrap();
        let acc_change = result
            .iter()
            .find(|c| c.owner == Owner::AddressOwner(acc_address))
            .unwrap();
        assert_eq!(gas_change.amount, -1000);
        assert_eq!(acc_change.amount, -200);
    }

    #[tokio::test]
    async fn test_failed_txn_coin_gas_and_accumulator_same_address() {
        let address = SuiAddress::random_for_testing_only();
        let (effects, provider) =
            create_failed_effects_with_gas_coin_and_accumulator(address, 1000, address, 200);

        let result = get_balance_changes_from_effect(&provider, &effects, vec![], None)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, Owner::AddressOwner(address));
        assert_eq!(result[0].coin_type, GAS::type_tag());
        assert_eq!(result[0].amount, -1200);
    }
}
