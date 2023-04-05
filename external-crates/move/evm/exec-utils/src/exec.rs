// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use evm::{
    backend::{Apply, ApplyBackend, Basic, MemoryAccount, MemoryBackend, MemoryVicinity},
    executor::stack::{MemoryStackState, StackExecutor, StackSubstateMetadata},
    Config, Context, CreateScheme, ExitReason, Runtime,
};
use primitive_types::{H160, U256};
use sha3::{Digest, Keccak256};
use std::{collections::BTreeMap, error::Error, fmt, rc::Rc};

// TODO: implement these features:
//   - Proper handling of gas

/// Stateful EVM executor backed by an in-memory storage.
pub struct Executor<'v> {
    storage_backend: MemoryBackend<'v>,
}

/// Return the 4-byte method selector derived from the signature, which is encoded as a string (e.g. `"foo(uint256,uint256)"`).
//
// TODO: Rust type to represent the signature.
pub fn derive_method_selector(sig: &str) -> [u8; 4] {
    let mut keccak = Keccak256::new();
    keccak.update(sig.as_bytes());
    let digest = keccak.finalize();
    [digest[0], digest[1], digest[2], digest[3]]
}

#[derive(Debug)]
pub enum MintError {
    BalanceOverflow,
}

impl fmt::Display for MintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct ExecuteResult {
    pub exit_reason: ExitReason,
    pub return_value: Vec<u8>,
    pub used_gas: u64,
}

impl fmt::Display for ExecuteResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} (used_gas={}): {:?}",
            self.exit_reason, self.used_gas, self.return_value
        )
    }
}

impl Error for MintError {}

impl<'v> Executor<'v> {
    /// Return a reference to the in-memory storage backend.
    pub fn storage(&self) -> &MemoryBackend<'v> {
        &self.storage_backend
    }

    /// Return a reference to the in-memory representation of the account at the specified adress if one exists.
    fn account_info(&self, address: H160) -> Option<&MemoryAccount> {
        self.storage_backend.state().get(&address)
    }

    /// Return the balance of the account at the specified address if one exists.
    pub fn account_balance(&self, address: H160) -> Option<U256> {
        self.account_info(address).map(|account| account.balance)
    }

    /// Add balance to the specified account.
    /// Intended to used for testing.
    ///
    /// Note: this will create a new account at the given address if one does not exist.
    pub fn mint(&mut self, address: H160, eth_amount: U256) -> Result<(), MintError> {
        let (balance, nonce) = match self.account_info(address) {
            Some(account) => {
                let balance = account
                    .balance
                    .checked_add(eth_amount)
                    .ok_or(MintError::BalanceOverflow)?;
                let nonce = account.nonce; // REVIEW: should the nonce be incremented?

                (balance, nonce)
            }
            None => (eth_amount, U256::from(0)),
        };

        self.storage_backend.apply(
            [Apply::Modify {
                address,
                basic: Basic { balance, nonce },
                code: None,
                storage: [],
                reset_storage: false,
            }],
            [],
            false,
        );

        Ok(())
    }

    /// Create a new `Executor` with an empty in-memory storage backend.
    //
    // TODO: review the lifetime of vicinity.
    pub fn new(vicinity: &'v MemoryVicinity) -> Self {
        Self {
            storage_backend: MemoryBackend::new(vicinity, BTreeMap::new()),
        }
    }

    // Perform a transaction and commit the changes to the storage backend.
    fn transact<F, R>(&mut self, op: F) -> R
    where
        F: for<'c> FnOnce(
            &mut StackExecutor<'c, 'static, MemoryStackState<'c, 'c, MemoryBackend<'v>>, ()>,
        ) -> R,
    {
        let config = Config::london();
        let metadata = StackSubstateMetadata::new(u64::MAX, &config);
        let state = MemoryStackState::new(metadata, &self.storage_backend);
        let mut exec = StackExecutor::new_with_precompiles(state, &config, &());

        let res = op(&mut exec);

        let state = exec.into_state();
        let (changes, logs) = state.deconstruct();
        self.storage_backend.apply(changes, logs, false);

        res
    }

    /// Create a contract and return the contract address if successful.
    pub fn create_contract(
        &mut self,
        caller_address: H160,
        contract_code: Vec<u8>,
    ) -> Result<H160, ExitReason> {
        self.transact(|exec| {
            let contract_address = exec.create_address(CreateScheme::Legacy {
                caller: caller_address,
            });

            let exit_reason = exec.transact_create(
                caller_address,
                U256::zero(), // TODO: allow the caller to specify this.
                contract_code,
                u64::MAX,
                vec![],
            );

            match &exit_reason {
                ExitReason::Succeed(_) => Ok(contract_address),
                _ => Err(exit_reason),
            }
        })
    }

    /// Transfer some ETH from one account to another.
    pub fn transfer_eth(
        &mut self,
        sender_address: H160,
        receipient_address: H160,
        eth_amount: U256,
    ) -> Result<(), ExitReason> {
        self.transact(|exec| {
            let (exit_reason, _buffer) = exec.transact_call(
                sender_address,
                receipient_address,
                eth_amount,
                vec![],
                u64::MAX,
                vec![],
            );

            match &exit_reason {
                ExitReason::Succeed(_) => Ok(()),
                _ => Err(exit_reason),
            }
        })
    }

    /// Call a contract method with the given signature.
    /// The signature is represented by a string consisting of the name of the method and
    /// a list of parameter types (e.g. `foo(uint256,uint256)`).
    pub fn call_function(
        &mut self,
        caller_address: H160,
        contract_address: H160,
        eth_amount: U256,
        method_sig: &str,
        method_args: &[u8],
    ) -> (ExitReason, Vec<u8>) {
        self.transact(|exec| {
            let mut data = vec![];
            data.extend(derive_method_selector(method_sig));
            data.extend(method_args);

            let (exit_reason, buffer) = exec.transact_call(
                caller_address,
                contract_address,
                eth_amount,
                data,
                u64::MAX,
                vec![],
            );

            (exit_reason, buffer)
        })
    }

    /// Execute custom EVM opecodes.
    /// You are still required to specify a caller address and a contract address, even though a contract may not exist
    /// at the specified address.
    pub fn execute_custom_code(
        &mut self,
        caller_address: H160,
        contract_address: H160,
        code: Vec<u8>,
        data: Vec<u8>,
    ) -> ExecuteResult {
        self.transact(|exec| {
            let context = Context {
                address: contract_address,
                caller: caller_address,
                apparent_value: U256::zero(), // TODO: figure out what this is.
            };
            let mut runtime = Runtime::new(Rc::new(code), Rc::new(data), context, exec.config());

            // REVIEW: are we handling gas metering correctly?
            let exit_reason = exec.execute(&mut runtime);
            let return_value = runtime.machine().return_value();
            let used_gas = exec.used_gas();
            ExecuteResult {
                exit_reason,
                return_value,
                used_gas,
            }
        })
    }
}
