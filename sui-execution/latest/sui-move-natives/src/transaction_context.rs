// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{account_address::AccountAddress, vm_status::StatusCode};
use move_vm_runtime::native_extensions::NativeExtensionMarker;
use std::{cell::RefCell, rc::Rc};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
};

// TransactionContext is a wrapper around TxContext that is exposed to NativeContextExtensions
// in order to provide transaction context information to Move native functions.
// Holds a Rc<RefCell<TxContext>> to allow for mutation of the TxContext.
#[derive(Tid)]
pub struct TransactionContext {
    pub(crate) tx_context: Rc<RefCell<TxContext>>,
    test_only: bool,
}

impl NativeExtensionMarker<'_> for TransactionContext {}

impl TransactionContext {
    pub fn new(tx_context: Rc<RefCell<TxContext>>) -> Self {
        Self {
            tx_context,
            test_only: false,
        }
    }

    pub fn new_for_testing(tx_context: Rc<RefCell<TxContext>>) -> Self {
        Self {
            tx_context,
            test_only: true,
        }
    }

    pub fn sender(&self) -> SuiAddress {
        self.tx_context.borrow().sender()
    }

    pub fn epoch(&self) -> EpochId {
        self.tx_context.borrow().epoch()
    }

    pub fn epoch_timestamp_ms(&self) -> u64 {
        self.tx_context.borrow().epoch_timestamp_ms()
    }

    pub fn digest(&self) -> TransactionDigest {
        self.tx_context.borrow().digest()
    }

    pub fn sponsor(&self) -> Option<SuiAddress> {
        self.tx_context.borrow().sponsor()
    }

    pub fn gas_price(&self) -> u64 {
        self.tx_context.borrow().gas_price()
    }

    pub fn gas_budget(&self) -> u64 {
        self.tx_context.borrow().gas_budget()
    }

    pub fn ids_created(&self) -> u64 {
        self.tx_context.borrow().ids_created()
    }

    pub fn fresh_id(&self) -> ObjectID {
        self.tx_context.borrow_mut().fresh_id()
    }

    //
    // Test only function
    //
    pub fn replace(
        &self,
        sender: AccountAddress,
        tx_hash: Vec<u8>,
        epoch: u64,
        epoch_timestamp_ms: u64,
        ids_created: u64,
        gas_price: u64,
        gas_budget: u64,
        sponsor: Option<AccountAddress>,
    ) -> PartialVMResult<()> {
        if !self.test_only {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("`replace` called on a non testing scenario".to_string()),
            );
        }
        self.tx_context.borrow_mut().replace(
            sender,
            tx_hash,
            epoch,
            epoch_timestamp_ms,
            ids_created,
            gas_price,
            gas_budget,
            sponsor,
        );
        Ok(())
    }
}
