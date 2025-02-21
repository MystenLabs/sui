// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_types::crypto::Signature;
use sui_types::move_package::UpgradePolicy;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;

use super::authority_test_utils::*;
use super::*;

pub fn build_test_modules_with_dep_addr(
    path: &Path,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_ids: impl IntoIterator<Item = (&'static str, ObjectID)>,
) -> CompiledPackage {
    let mut build_config = BuildConfig::new_for_testing();
    for (addr_name, obj_id) in dep_original_addresses {
        build_config
            .config
            .additional_named_addresses
            .insert(addr_name.to_string(), AccountAddress::from(obj_id));
    }
    let mut package = build_config.build(path).unwrap();

    let dep_id_mapping: BTreeMap<_, _> = dep_ids
        .into_iter()
        .map(|(dep_name, obj_id)| (Symbol::from(dep_name), obj_id))
        .collect();

    assert_eq!(
        dep_id_mapping.len(),
        package.dependency_ids.unpublished.len()
    );
    for unpublished_dep in &package.dependency_ids.unpublished {
        let published_id = dep_id_mapping.get(unpublished_dep).unwrap();
        // Make sure we aren't overriding a package
        assert!(package
            .dependency_ids
            .published
            .insert(*unpublished_dep, *published_id)
            .is_none())
    }

    // No unpublished deps
    package.dependency_ids.unpublished.clear();
    package
}

/// Returns the new package's ID and the upgrade cap object ref.
/// `dep_original_addresses` allows us to fill out mappings in the addresses section of the package manifest. These IDs
/// must be the original IDs of names.
/// dep_ids are the IDs of the dependencies of the package, in the latest version (if there were upgrades).
pub async fn publish_package_on_single_authority(
    path: &Path,
    sender: SuiAddress,
    sender_key: &dyn Signer<Signature>,
    gas_payment: ObjectRef,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_ids: Vec<ObjectID>,
    state: &Arc<AuthorityState>,
) -> SuiResult<(TransactionDigest, (ObjectID, ObjectRef))> {
    let mut build_config = BuildConfig::new_for_testing();
    for (addr_name, obj_id) in dep_original_addresses {
        build_config
            .config
            .additional_named_addresses
            .insert(addr_name.to_string(), AccountAddress::from(obj_id));
    }
    let modules = build_config.build(path).unwrap().get_package_bytes(false);

    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder.publish_upgradeable(modules, dep_ids);
    builder.transfer_arg(sender, cap);
    let pt = builder.finish();

    let rgp = state.epoch_store_for_testing().reference_gas_price();
    let txn_data = TransactionData::new_programmable(
        sender,
        vec![gas_payment],
        pt,
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );

    let signed = to_sender_signed_transaction(txn_data, sender_key);
    let (_cert, effects) = send_and_confirm_transaction(state, signed).await?;
    assert!(effects.data().status().is_ok());
    let package_id = effects
        .data()
        .created()
        .iter()
        .find(|c| c.1 == Owner::Immutable)
        .unwrap()
        .0
         .0;
    let cap_object = effects
        .data()
        .created()
        .iter()
        .find(|c| matches!(c.1, Owner::AddressOwner(..)))
        .unwrap()
        .0;
    Ok((*effects.transaction_digest(), (package_id, cap_object)))
}

pub async fn upgrade_package_on_single_authority(
    path: &Path,
    sender: SuiAddress,
    sender_key: &dyn Signer<Signature>,
    gas_payment: ObjectRef,
    package_id: ObjectID,
    upgrade_cap: ObjectRef,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_id_mapping: impl IntoIterator<Item = (&'static str, ObjectID)>,
    state: &Arc<AuthorityState>,
) -> SuiResult<(TransactionDigest, ObjectID)> {
    let package = build_test_modules_with_dep_addr(path, dep_original_addresses, dep_id_mapping);

    let with_unpublished_deps = false;
    let modules = package.get_package_bytes(with_unpublished_deps);
    let digest = package.get_package_digest(with_unpublished_deps).to_vec();

    let rgp = state.epoch_store_for_testing().reference_gas_price();
    let data = TransactionData::new_upgrade(
        sender,
        gas_payment,
        package_id,
        modules,
        package.get_dependency_storage_package_ids(),
        (upgrade_cap, Owner::AddressOwner(sender)),
        UpgradePolicy::COMPATIBLE,
        digest,
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    )
    .unwrap();
    let signed = to_sender_signed_transaction(data, sender_key);
    let (_cert, effects) = send_and_confirm_transaction(state, signed).await?;
    assert!(effects.data().status().is_ok());
    let package_id = effects
        .data()
        .created()
        .iter()
        .find(|c| c.1 == Owner::Immutable)
        .unwrap()
        .0
         .0;
    Ok((*effects.transaction_digest(), package_id))
}
