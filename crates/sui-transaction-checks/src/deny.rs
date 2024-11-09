// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto_zkp::bn254::zk_login::OIDCProvider;
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_types::{
    base_types::ObjectRef,
    error::{SuiError, SuiResult, UserInputError},
    signature::GenericSignature,
    storage::BackingPackageStore,
    transaction::{Command, InputObjectKind, TransactionData, TransactionDataAPI},
};
macro_rules! deny_if_true {
    ($cond:expr, $msg:expr) => {
        if ($cond) {
            return Err(SuiError::UserInputError {
                error: UserInputError::TransactionDenied {
                    error: $msg.to_string(),
                },
            });
        }
    };
}

/// Check that the provided transaction is allowed to be signed according to the
/// deny config.
pub fn check_transaction_for_signing(
    tx_data: &TransactionData,
    tx_signatures: &[GenericSignature],
    input_object_kinds: &[InputObjectKind],
    receiving_objects: &[ObjectRef],
    filter_config: &TransactionDenyConfig,
    package_store: &dyn BackingPackageStore,
) -> SuiResult {
    check_disabled_features(filter_config, tx_data, tx_signatures)?;

    check_signers(filter_config, tx_data)?;

    check_input_objects(filter_config, input_object_kinds)?;

    check_package_dependencies(filter_config, tx_data, package_store)?;

    check_receiving_objects(filter_config, receiving_objects)?;

    Ok(())
}

fn check_receiving_objects(
    filter_config: &TransactionDenyConfig,
    receiving_objects: &[ObjectRef],
) -> SuiResult {
    deny_if_true!(
        filter_config.receiving_objects_disabled() && !receiving_objects.is_empty(),
        "Receiving objects is temporarily disabled".to_string()
    );
    for (id, _, _) in receiving_objects {
        deny_if_true!(
            filter_config.get_object_deny_set().contains(id),
            format!("Access to object {:?} is temporarily disabled", id)
        );
    }
    Ok(())
}

fn check_disabled_features(
    filter_config: &TransactionDenyConfig,
    tx_data: &TransactionData,
    tx_signatures: &[GenericSignature],
) -> SuiResult {
    deny_if_true!(
        filter_config.user_transaction_disabled(),
        "Transaction signing is temporarily disabled"
    );

    tx_signatures.iter().try_for_each(|s| {
        if let GenericSignature::ZkLoginAuthenticator(z) = s {
            deny_if_true!(
                filter_config.zklogin_sig_disabled(),
                "zkLogin authenticator is temporarily disabled"
            );
            deny_if_true!(
                filter_config.zklogin_disabled_providers().contains(
                    &OIDCProvider::from_iss(z.get_iss())
                        .map_err(|_| SuiError::UnexpectedMessage(z.get_iss().to_string()))?
                        .to_string()
                ),
                "zkLogin OAuth provider is temporarily disabled"
            )
        }
        Ok(())
    })?;

    if !filter_config.package_publish_disabled() && !filter_config.package_upgrade_disabled() {
        return Ok(());
    }

    for command in tx_data.kind().iter_commands() {
        deny_if_true!(
            filter_config.package_publish_disabled() && matches!(command, Command::Publish(..)),
            "Package publish is temporarily disabled"
        );
        deny_if_true!(
            filter_config.package_upgrade_disabled() && matches!(command, Command::Upgrade(..)),
            "Package upgrade is temporarily disabled"
        );
    }
    Ok(())
}

fn check_signers(filter_config: &TransactionDenyConfig, tx_data: &TransactionData) -> SuiResult {
    let deny_map = filter_config.get_address_deny_set();
    if deny_map.is_empty() {
        return Ok(());
    }
    for signer in tx_data.signers() {
        deny_if_true!(
            deny_map.contains(&signer),
            format!(
                "Access to account address {:?} is temporarily disabled",
                signer
            )
        );
    }
    Ok(())
}

fn check_input_objects(
    filter_config: &TransactionDenyConfig,
    input_object_kinds: &[InputObjectKind],
) -> SuiResult {
    let deny_map = filter_config.get_object_deny_set();
    let shared_object_disabled = filter_config.shared_object_disabled();
    if deny_map.is_empty() && !shared_object_disabled {
        // No need to iterate through the input objects if no relevant policy is set.
        return Ok(());
    }
    for input_object_kind in input_object_kinds {
        let id = input_object_kind.object_id();
        deny_if_true!(
            deny_map.contains(&id),
            format!("Access to input object {:?} is temporarily disabled", id)
        );
        deny_if_true!(
            shared_object_disabled && input_object_kind.is_shared_object(),
            "Usage of shared object in transactions is temporarily disabled"
        );
    }
    Ok(())
}

fn check_package_dependencies(
    filter_config: &TransactionDenyConfig,
    tx_data: &TransactionData,
    package_store: &dyn BackingPackageStore,
) -> SuiResult {
    let deny_map = filter_config.get_package_deny_set();
    if deny_map.is_empty() {
        return Ok(());
    }
    let mut dependencies = vec![];
    for command in tx_data.kind().iter_commands() {
        match command {
            Command::Publish(_, deps) => {
                // It is possible that the deps list is inaccurate since it's provided
                // by the user. But that's OK because this publish transaction will fail
                // to execute in the end. Similar reasoning for Upgrade.
                dependencies.extend(deps.iter().copied());
            }
            Command::Upgrade(_, deps, package_id, _) => {
                dependencies.extend(deps.iter().copied());
                // It's crucial that we don't allow upgrading a package in the deny list,
                // otherwise one can bypass the deny list by upgrading a package.
                dependencies.push(*package_id);
            }
            Command::MoveCall(call) => {
                let package = package_store.get_package_object(&call.package)?.ok_or(
                    SuiError::UserInputError {
                        error: UserInputError::ObjectNotFound {
                            object_id: call.package,
                            version: None,
                        },
                    },
                )?;
                // linkage_table maps from the original package ID to the upgraded ID for each
                // dependency. Here we only check the upgraded (i.e. the latest) ID against the
                // deny list. This means that we only make sure that the denied package is not
                // currently used as a dependency. This allows us to deny an older version of
                // package but permits the use of a newer version.
                dependencies.extend(
                    package
                        .move_package()
                        .linkage_table()
                        .values()
                        .map(|upgrade_info| upgrade_info.upgraded_id),
                );
                dependencies.push(package.move_package().id());
            }
            Command::TransferObjects(..)
            | &Command::SplitCoins(..)
            | &Command::MergeCoins(..)
            | &Command::MakeMoveVec(..) => {}
        }
    }
    for dep in dependencies {
        deny_if_true!(
            deny_map.contains(&dep),
            format!("Access to package {:?} is temporarily disabled", dep)
        );
    }
    Ok(())
}
