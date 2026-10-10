// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Admission-time account policy checks. Execution enforces the policy authoritatively; these
//! checks let a validator refuse to sign a transaction that cannot pass, so the rejection costs
//! the sender no gas. They read the latest state rather than a sequenced version, which is fine
//! for a local decision: a transaction that slips through here is still checked at execution.

use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    account_policy::{AccountPolicy, account_policy_field_id, counter_address, spent_type_tag},
    accumulator_root::{AccumulatorValue, UnsettledObjectFundsRead},
    error::{SuiResult, UserInputError},
    execution_status::AccountPolicyViolationKind,
    gas_coin::GAS,
    storage::BackingPackageStore,
    transaction::{Command, SenderSignedData, TransactionDataAPI, TransactionKind},
};

use crate::authority::{AuthorityPerEpochStore, AuthorityState};

impl AuthorityState {
    pub(crate) fn handle_account_policy_checks(
        &self,
        transaction: &SenderSignedData,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        if !epoch_store.protocol_config().enable_account_policy() {
            return Ok(());
        }
        let tx_data = transaction.transaction_data();
        let TransactionKind::ProgrammableTransaction(pt) = tx_data.kind() else {
            return Ok(());
        };
        let sender = tx_data.sender();
        let store = self.get_backing_store();
        let Some(registry) = store.get_object(&SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID) else {
            return Ok(());
        };
        let Some(field) = store.read_child_object(
            &SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
            &account_policy_field_id(sender),
            registry.version(),
        )?
        else {
            return Ok(());
        };
        let Some(policy) = AccountPolicy::from_field_object(&field) else {
            return Ok(());
        };
        let epoch = epoch_store.epoch();
        if !policy.is_active(epoch) || policy.is_guardian_approved(&transaction.co_signers()) {
            return Ok(());
        }
        let violation = |kind| Err(UserInputError::AccountPolicyViolation { kind }.into());

        let gas_budget = tx_data.gas_budget();
        if gas_budget > policy.gas_budget_cap {
            return violation(AccountPolicyViolationKind::GasBudgetExceeded);
        }
        for command in &pt.commands {
            match command {
                Command::MoveCall(call) => {
                    let original = self
                        .get_backing_package_store()
                        .get_package_object(&call.package)?
                        .map(|package| package.move_package().original_package_id());
                    if !original.is_some_and(|original| policy.allows_package(original)) {
                        return violation(AccountPolicyViolationKind::PackageNotAllowed);
                    }
                }
                Command::Publish(..) | Command::Upgrade(..) => {
                    return violation(AccountPolicyViolationKind::PublishNotAllowed);
                }
                _ => {}
            }
        }

        // The gas budget alone must fit in what is left of this epoch's SUI budget.
        let Some(root) = store.get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID) else {
            return Ok(());
        };
        let address = counter_address(sender, epoch, None);
        let sui_spent_type = spent_type_tag(GAS::type_tag());
        let settled = AccumulatorValue::load(
            store.as_ref(),
            Some(root.version()),
            address,
            &sui_spent_type,
        )?
        .and_then(|value| value.as_u128())
        .unwrap_or(0);
        let unsettled = AccumulatorValue::get_field_id(address, &sui_spent_type)
            .map(|field_id| {
                self.unsettled_object_withdrawals
                    .get_unsettled_counter_merge(&field_id, root.version())
            })
            .unwrap_or(0);
        if settled + unsettled + u128::from(gas_budget)
            > u128::from(policy.coin_limit(&GAS::type_tag()))
        {
            return violation(AccountPolicyViolationKind::CoinOutflowExceeded);
        }
        Ok(())
    }
}
