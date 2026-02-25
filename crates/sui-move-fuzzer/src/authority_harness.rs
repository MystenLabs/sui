// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::identifier::Identifier;
use once_cell::sync::Lazy;
use sui_core::authority::authority_test_utils::submit_and_execute;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Command, TransactionData};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID};

/// Default gas balance for fuzzing gas objects.
const FUZZ_GAS_BALANCE: u64 = 300_000_000_000_000;

/// Default gas budget per transaction.
const FUZZ_GAS_BUDGET: u64 = 500_000_000;

/// Wraps a real Sui validator for end-to-end fuzz testing.
pub struct FuzzAuthority {
    runtime: tokio::runtime::Runtime,
    authority: Arc<AuthorityState>,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object_id: ObjectID,
}

/// Global singleton so all fuzz iterations share one authority.
pub static FUZZ_AUTHORITY: Lazy<FuzzAuthority> = Lazy::new(FuzzAuthority::init);

impl FuzzAuthority {
    /// Build a test authority, generate a sender keypair, and insert a gas object.
    pub fn init() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let (sender, sender_key): (SuiAddress, AccountKeyPair) =
            sui_types::crypto::deterministic_random_account_key();
        let gas_object_id = ObjectID::random();

        let authority = runtime.block_on(async {
            let state = TestAuthorityBuilder::new().build().await;
            let gas_object =
                Object::with_id_owner_gas_for_testing(gas_object_id, sender, FUZZ_GAS_BALANCE);
            state.insert_genesis_object(gas_object).await;
            state
        });

        FuzzAuthority {
            runtime,
            authority,
            sender,
            sender_key,
            gas_object_id,
        }
    }

    /// Look up the current gas object reference from the authority store.
    fn gas_object_ref(&self) -> ObjectRef {
        self.runtime
            .block_on(async {
                self.authority
                    .get_object(&self.gas_object_id)
                    .await
                    .expect("gas object must exist")
                    .compute_object_reference()
            })
    }

    /// Reference gas price from the authority's epoch store.
    fn reference_gas_price(&self) -> u64 {
        self.authority
            .reference_gas_price_for_testing()
            .expect("rgp must be available")
    }

    /// Publish a module (as raw bytes) and return the package ObjectID and effects.
    pub fn publish_module(
        &self,
        module_bytes: Vec<u8>,
    ) -> Result<(ObjectID, TransactionEffects), String> {
        let gas_ref = self.gas_object_ref();
        let rgp = self.reference_gas_price();

        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(
            vec![module_bytes],
            vec![MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID],
        );
        let pt = builder.finish();

        let data = TransactionData::new_programmable(
            self.sender,
            vec![gas_ref],
            pt,
            FUZZ_GAS_BUDGET,
            rgp,
        );
        let transaction = to_sender_signed_transaction(data, &self.sender_key);

        let (_exec, signed_effects) = self
            .runtime
            .block_on(submit_and_execute(&self.authority, transaction))
            .map_err(|e| format!("submit_and_execute failed: {e}"))?;

        let effects = signed_effects.into_data();

        if !effects.status().is_ok() {
            return Err(format!("publish failed: {:?}", effects.status()));
        }

        // The first created object owned by Immutable is the package.
        let package_id = effects
            .created()
            .iter()
            .find(|(_, owner)| owner.is_immutable())
            .map(|(obj_ref, _)| obj_ref.0)
            .ok_or_else(|| "no immutable package object in effects".to_string())?;

        Ok((package_id, effects))
    }

    /// Call an entry function with no arguments on a published package.
    pub fn call_entry_function(
        &self,
        package: ObjectID,
        module: &str,
        function: &str,
    ) -> Result<TransactionEffects, String> {
        let gas_ref = self.gas_object_ref();
        let rgp = self.reference_gas_price();

        let mut builder = ProgrammableTransactionBuilder::new();
        builder.command(Command::move_call(
            package,
            Identifier::new(module).map_err(|e| format!("bad module name: {e}"))?,
            Identifier::new(function).map_err(|e| format!("bad function name: {e}"))?,
            vec![],
            vec![],
        ));
        let pt = builder.finish();

        let data = TransactionData::new_programmable(
            self.sender,
            vec![gas_ref],
            pt,
            FUZZ_GAS_BUDGET,
            rgp,
        );
        let transaction = to_sender_signed_transaction(data, &self.sender_key);

        let (_exec, signed_effects) = self
            .runtime
            .block_on(submit_and_execute(&self.authority, transaction))
            .map_err(|e| format!("submit_and_execute failed: {e}"))?;

        Ok(signed_effects.into_data())
    }

    /// Create a fresh gas object for the next iteration (the old one may have been consumed
    /// or mutated by a previous transaction).
    pub fn refresh_gas(&mut self) {
        let new_gas_id = ObjectID::random();
        let gas_object =
            Object::with_id_owner_gas_for_testing(new_gas_id, self.sender, FUZZ_GAS_BALANCE);
        self.runtime
            .block_on(self.authority.insert_genesis_object(gas_object));
        self.gas_object_id = new_gas_id;
    }
}
