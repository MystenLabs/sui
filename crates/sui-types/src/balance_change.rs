// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::base_types::SequenceNumber;
use crate::base_types::SuiAddress;
use crate::coin::Coin;
use crate::effects::IDOperation;
use crate::effects::TransactionEffects;
use crate::effects::TransactionEffectsAPI;
use crate::effects::UnchangedConsensusKind;
use crate::gas_coin::GAS;
use crate::id::UID;
use crate::object::balance_traversal::BalanceTraversal;
use crate::object::Object;
use crate::object::Owner;
use crate::storage::{BackingPackageStore, ParentSync};
use crate::storage::{ChildObjectResolver, PackageObject};
use crate::transaction::EndOfEpochTransactionKind;
use crate::transaction::TransactionData;
use crate::transaction::TransactionDataAPI;
use anyhow::bail;
use anyhow::Context;
use move_core_types::language_storage::TypeTag;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveTypeLayout, MoveValue},
    annotated_visitor as AV,
};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use tracing::debug;

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

pub fn derive_balance_changes(
    _effects: &TransactionEffects,
    input_objects: &[Object],
    output_objects: &[Object],
) -> Vec<BalanceChange> {
    // 1. subtract all input coins
    let balances = coins(input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin_type, balance)| {
            *acc.entry((address, coin_type)).or_default() -= balance as i128;
            acc
        },
    );

    // 2. add all mutated/output coins
    let balances =
        coins(output_objects).fold(balances, |mut acc, (address, coin_type, balance)| {
            *acc.entry((address, coin_type)).or_default() += balance as i128;
            acc
        });

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address: *address,
                coin_type,
                amount,
            })
        })
        .collect()
}

//
// A BackingStore to pass to execution in order to track all objects loaded during execution to
// later be used for balance change calculations
//
pub struct TrackingBackingStore<'a> {
    inner: &'a dyn crate::storage::BackingStore,
    read_objects: std::cell::RefCell<BTreeMap<ObjectID, BTreeMap<u64, Object>>>,
}

impl<'a> TrackingBackingStore<'a> {
    pub fn new(inner: &'a dyn crate::storage::BackingStore) -> Self {
        Self {
            inner,
            read_objects: Default::default(),
        }
    }

    pub fn into_read_objects(self) -> BTreeMap<ObjectID, BTreeMap<u64, Object>> {
        self.read_objects.into_inner()
    }

    fn track_object(&self, object: &Object) {
        let id = object.id();
        let version = object.version().value();
        self.read_objects
            .borrow_mut()
            .entry(id)
            .or_default()
            .insert(version, object.clone());
    }
}

impl BackingPackageStore for TrackingBackingStore<'_> {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> crate::error::SuiResult<Option<PackageObject>> {
        self.inner.get_package_object(package_id).inspect(|o| {
            o.as_ref()
                .inspect(|package| self.track_object(package.object()));
        })
    }
}

impl ChildObjectResolver for TrackingBackingStore<'_> {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> crate::error::SuiResult<Option<Object>> {
        self.inner
            .read_child_object(parent, child, child_version_upper_bound)
            .inspect(|o| {
                o.as_ref().inspect(|object| self.track_object(object));
            })
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: crate::committee::EpochId,
    ) -> crate::error::SuiResult<Option<Object>> {
        self.inner
            .get_object_received_at_version(
                owner,
                receiving_object_id,
                receive_object_at_version,
                epoch_id,
            )
            .inspect(|o| {
                o.as_ref().inspect(|object| self.track_object(object));
            })
    }
}

impl crate::storage::ObjectStore for TrackingBackingStore<'_> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.inner
            .get_object(object_id)
            .inspect(|o| self.track_object(o))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: crate::base_types::VersionNumber,
    ) -> Option<Object> {
        self.inner
            .get_object_by_key(object_id, version)
            .inspect(|o| self.track_object(o))
    }
}

impl ParentSync for TrackingBackingStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        object_id: ObjectID,
    ) -> Option<crate::base_types::ObjectRef> {
        self.inner.get_latest_parent_entry_ref_deprecated(object_id)
    }
}

//
// True balance change calculation
//
pub fn calculate_balance_changes(
    certificate: &crate::executable_transaction::VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    inner_temporary_store: &crate::inner_temporary_store::InnerTemporaryStore,
    mut layout_resolver: Box<dyn crate::layout_resolver::LayoutResolver + '_>,
    tracked_objects: TrackingBackingStore<'_>,
) -> anyhow::Result<Vec<BalanceChange>> {
    let mut object_cache = tracked_objects.into_read_objects();

    // dump input objects into the cache
    for object in inner_temporary_store.input_objects.values() {
        let id = object.id();
        let version = object.version().value();
        object_cache
            .entry(id)
            .or_default()
            .insert(version, object.clone());
    }

    let mut unchanged = BTreeMap::new();

    // We start with adding all loaded runtime objects to the set of "unchanged" even though this
    // isn't completely accurate as some of these objects may get deleted and will be cleaned up
    // when going through changed objects
    for (id, meta) in &inner_temporary_store.loaded_runtime_objects {
        let obj = object_cache
            .get(id)
            .and_then(|vs| vs.get(&meta.version.value()).cloned())
            .context("Loaded runtime object not in cache")?;

        unchanged.insert(*id, obj);
    }

    for (id, kind) in effects.unchanged_consensus_objects() {
        let UnchangedConsensusKind::ReadOnlyRoot((v, _)) = kind else {
            continue;
        };

        let obj = object_cache
            .get(&id)
            .and_then(|vs| vs.get(&v.value()).cloned())
            .context("Unchanged consensus object not in cache")?;

        unchanged.insert(id, obj);
    }

    let mut reads = unchanged.clone();
    let mut writes = unchanged.clone();
    for change in effects.object_changes() {
        if let Some(input) = change.input_version {
            reads.insert(
                change.id,
                object_cache
                    .get(&change.id)
                    .and_then(|vs| vs.get(&input.value()).cloned())
                    .context("Input object not in cache")?,
            );
        };

        if let Some(output) = change.output_version {
            let object = inner_temporary_store
                .written
                .get(&change.id)
                .context("Written object not in inner store")?;

            assert!(
                object.version() == output,
                "Written object version mismatch"
            );

            writes.insert(change.id, object.clone());
        };

        // if an object was deleted we need to remove it from the write set since it may have been
        // added above as a loaded runtime object.
        if matches!(change.id_operation, IDOperation::Deleted) {
            writes.remove(&change.id);
        }
    }

    let address_balance_changes = balance_changes(
        certificate.transaction_data(),
        effects,
        &reads,
        &writes,
        &mut layout_resolver,
    )
    .context("Failed to compute balance changes")?;

    // If debug level tracing is enabled for this module then we can do some more expensive work to
    // call out changes that are non-conservative (don't net out to 0, which can happen if tokens
    // were minted or burned) as well as display the balance change itself.
    if tracing::enabled!(tracing::Level::DEBUG) {
        let mut balance_changes = BTreeMap::new();
        for change in &address_balance_changes {
            *balance_changes.entry(change.coin_type.clone()).or_insert(0) += change.amount;
        }

        for (coin_type, amount) in balance_changes {
            if amount != 0 {
                debug!(
                    "{} not conserved: {amount}",
                    coin_type.to_canonical_display(true)
                );
            }
        }

        if !address_balance_changes.is_empty() {
            debug!("Balance Changes: {address_balance_changes:#?}",);
        }
    }

    Ok(address_balance_changes)
}

fn balance_changes(
    transaction: &TransactionData,
    effects: &TransactionEffects,
    read: &BTreeMap<ObjectID, Object>,
    write: &BTreeMap<ObjectID, Object>,
    layout_resolver: &mut Box<dyn crate::layout_resolver::LayoutResolver + '_>,
) -> anyhow::Result<Vec<BalanceChange>> {
    let mut balance_in = root_balances(read, layout_resolver)?;
    let mut balance_out = root_balances(write, layout_resolver)?;

    // For ChangeEpoch txns we need to attribute the amount that has been "paid in" (storage charge
    // and computation_charge) and "paid out" (storage rebates) to the system throughout the epoch.
    if let Some(change_epoch) = match transaction.kind() {
        crate::transaction::TransactionKind::ChangeEpoch(change_epoch) => Some(change_epoch),
        crate::transaction::TransactionKind::EndOfEpochTransaction(eoe) => {
            eoe.iter().find_map(|t| {
                if let EndOfEpochTransactionKind::ChangeEpoch(change_epoch) = t {
                    Some(change_epoch)
                } else {
                    None
                }
            })
        }
        _ => None,
    } {
        let accumulated_balance =
            // Storage charge and computation charge are minted at the beginning of the ChangeEpoch
            // transaction (and were collected and accumulated during the epoch).
            change_epoch.storage_charge as i128
            + change_epoch.computation_charge as i128
            // Storage rebates are burned at the end of the ChangeEpoch transaction (and were
            // technically already paid out to users).
            - change_epoch.storage_rebate as i128;
        *balance_in
            .entry((crate::SUI_SYSTEM_STATE_ADDRESS.into(), GAS::type_().into()))
            .or_insert(0) += accumulated_balance;
    }

    // Attribute gas costs to the system object
    *balance_out
        .entry((crate::SUI_SYSTEM_STATE_ADDRESS.into(), GAS::type_().into()))
        .or_insert(0) += effects.gas_cost_summary().net_gas_usage() as i128;

    let mut balance_changes: BTreeMap<(SuiAddress, TypeTag), i128> = BTreeMap::new();
    for ((address, coin_type), amount) in balance_out {
        *balance_changes.entry((address, coin_type)).or_insert(0) += amount;
    }

    for ((address, coin_type), amount) in balance_in {
        *balance_changes.entry((address, coin_type)).or_insert(0) -= amount;
    }

    Ok(balance_changes
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            (amount != 0).then_some(BalanceChange {
                address,
                coin_type,
                amount,
            })
        })
        .collect())
}

fn root_balances(
    working_set: &BTreeMap<ObjectID, Object>,
    layout_resolver: &mut Box<dyn crate::layout_resolver::LayoutResolver + '_>,
) -> anyhow::Result<BTreeMap<(SuiAddress, TypeTag), i128>> {
    // Traverse each object to find the UIDs and balances it wraps.
    let mut wrapper = BTreeMap::new();
    let mut object_balances = BTreeMap::new();
    for (id, obj) in working_set {
        let Some(obj) = obj.data.try_as_move() else {
            continue;
        };

        let layout = layout_resolver
            .get_annotated_layout(&obj.type_().clone().into())?
            .into_layout();
        object_balances.insert(*id, wrapped_balances(&layout, obj.contents())?);

        for child in wrapped_uids(&layout, obj.contents())? {
            wrapper.insert(child, *id);
        }
    }

    // Work back through wrappers to associate each object with a root owning address. This walks
    // back through parent -> child object relationships (dynamic fields) until it finds an
    // address-owner, or a shared or immutable object.
    let mut root_owners = BTreeMap::new();
    for (id, mut obj) in working_set {
        let mut curr = *id;
        loop {
            match obj.owner() {
                Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                    root_owners.insert(*id, *owner);
                    break;
                }

                Owner::Immutable | Owner::Shared { .. } => {
                    root_owners.insert(*id, curr.into());
                    break;
                }

                Owner::ObjectOwner(address) => {
                    let mut next = ObjectID::from(*address);
                    if let Some(parent) = wrapper.get(&next) {
                        next = *parent;
                    }

                    let Some(next_obj) = working_set.get(&next) else {
                        bail!("Cannot find owner of {curr} in the working set: {next}");
                    };

                    curr = next;
                    obj = next_obj;
                }
            }
        }
    }

    // Accumulate balance changes to root owners.
    let mut balances = BTreeMap::new();
    for (id, obj_balances) in object_balances {
        let Some(root) = root_owners.get(&id) else {
            bail!("Cannot find root owner of {id} in the working set");
        };

        for (coin_type, amount) in obj_balances {
            *balances.entry((*root, coin_type)).or_insert(0) += amount as i128;
        }
    }

    Ok(balances)
}

fn wrapped_balances(
    layout: &MoveTypeLayout,
    contents: &[u8],
) -> anyhow::Result<BTreeMap<TypeTag, u64>> {
    let mut visitor = BalanceTraversal::default();
    MoveValue::visit_deserialize(contents, layout, &mut visitor)?;
    Ok(visitor.finish())
}

fn wrapped_uids(layout: &MoveTypeLayout, contents: &[u8]) -> anyhow::Result<BTreeSet<ObjectID>> {
    let mut ids = BTreeSet::new();
    struct UIDTraversal<'i>(&'i mut BTreeSet<ObjectID>);
    struct UIDCollector<'i>(&'i mut BTreeSet<ObjectID>);

    impl<'b, 'l> AV::Traversal<'b, 'l> for UIDTraversal<'_> {
        type Error = AV::Error;

        fn traverse_struct(
            &mut self,
            driver: &mut AV::StructDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            if driver.struct_layout().type_ == UID::type_() {
                while driver.next_field(&mut UIDCollector(self.0))?.is_some() {}
            } else {
                while driver.next_field(self)?.is_some() {}
            }
            Ok(())
        }
    }

    impl<'b, 'l> AV::Traversal<'b, 'l> for UIDCollector<'_> {
        type Error = AV::Error;
        fn traverse_address(
            &mut self,
            _driver: &AV::ValueDriver<'_, 'b, 'l>,
            value: AccountAddress,
        ) -> Result<(), Self::Error> {
            self.0.insert(value.into());
            Ok(())
        }
    }

    MoveValue::visit_deserialize(contents, layout, &mut UIDTraversal(&mut ids))?;
    Ok(ids)
}
