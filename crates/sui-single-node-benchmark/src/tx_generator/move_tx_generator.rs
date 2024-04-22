// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
use crate::tx_generator::TxGenerator;
use move_core_types::identifier::Identifier;
use std::collections::HashMap;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, Transaction, DEFAULT_VALIDATOR_GAS_PRICE};

pub struct MoveTxGenerator {
    move_package: ObjectID,
    num_transfers: u64,
    use_native_transfer: bool,
    computation: u8,
    root_objects: HashMap<SuiAddress, ObjectRef>,
    shared_objects: Vec<(ObjectID, SequenceNumber)>,
    num_mints: u16,
    nft_size: u16,
    use_batch_mint: bool,
}

impl MoveTxGenerator {
    pub fn new(
        move_package: ObjectID,
        num_transfers: u64,
        use_native_transfer: bool,
        computation: u8,
        root_objects: HashMap<SuiAddress, ObjectRef>,
        shared_objects: Vec<(ObjectID, SequenceNumber)>,
        num_mints: u16,
        nft_size: u16,
        use_batch_mint: bool,
    ) -> Self {
        Self {
            move_package,
            num_transfers,
            use_native_transfer,
            computation,
            root_objects,
            shared_objects,
            num_mints,
            nft_size,
            use_batch_mint,
        }
    }
}

impl TxGenerator for MoveTxGenerator {
    fn generate_tx(&self, account: Account) -> Transaction {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            // Step 1: transfer `num_transfers` objects.
            // First object in the gas_objects is the gas object and we are not transferring it.
            for i in 1..=self.num_transfers {
                let object = account.gas_objects[i as usize];
                if self.use_native_transfer {
                    builder.transfer_object(account.sender, object).unwrap();
                } else {
                    builder
                        .move_call(
                            self.move_package,
                            Identifier::new("benchmark").unwrap(),
                            Identifier::new("transfer_coin").unwrap(),
                            vec![],
                            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(object))],
                        )
                        .unwrap();
                }
            }
            for shared_object in &self.shared_objects {
                builder
                    .move_call(
                        self.move_package,
                        Identifier::new("benchmark").unwrap(),
                        Identifier::new("increment_shared_counter").unwrap(),
                        vec![],
                        vec![CallArg::Object(ObjectArg::SharedObject {
                            id: shared_object.0,
                            initial_shared_version: shared_object.1,
                            mutable: true,
                        })],
                    )
                    .unwrap();
            }

            if !self.root_objects.is_empty() {
                // Step 2: Read all dynamic fields from the root object.
                let root_object = self.root_objects.get(&account.sender).unwrap();
                let root_object_arg = builder
                    .obj(ObjectArg::ImmOrOwnedObject(*root_object))
                    .unwrap();
                builder.programmable_move_call(
                    self.move_package,
                    Identifier::new("benchmark").unwrap(),
                    Identifier::new("read_dynamic_fields").unwrap(),
                    vec![],
                    vec![root_object_arg],
                );
            }

            if self.computation > 0 {
                // Step 3: Run some computation.
                let computation_arg = builder.pure(self.computation as u64 * 100).unwrap();
                builder.programmable_move_call(
                    self.move_package,
                    Identifier::new("benchmark").unwrap(),
                    Identifier::new("run_computation").unwrap(),
                    vec![],
                    vec![computation_arg],
                );
            }
            if self.num_mints > 0 {
                // Step 4: Mint some NFTs
                let mut contents = Vec::new();
                assert!(self.nft_size >= 32, "NFT size must be at least 32 bytes");
                for _ in 0..self.nft_size - 32 {
                    contents.push(7u8)
                }
                if self.use_batch_mint {
                    // create a vector of sender addresses to pass to batch_mint
                    let mut recipients = Vec::new();
                    for _ in 0..self.num_mints {
                        recipients.push(account.sender)
                    }
                    let args = vec![
                        builder.pure(recipients).unwrap(),
                        builder.pure(contents).unwrap(),
                    ];
                    builder.programmable_move_call(
                        self.move_package,
                        Identifier::new("benchmark").unwrap(),
                        Identifier::new("batch_mint").unwrap(),
                        vec![],
                        args,
                    );
                } else {
                    // create PTB with a command that transfers each
                    for _ in 0..self.num_mints {
                        let args = vec![
                            builder.pure(account.sender).unwrap(),
                            builder.pure(contents.clone()).unwrap(),
                        ];
                        builder.programmable_move_call(
                            self.move_package,
                            Identifier::new("benchmark").unwrap(),
                            Identifier::new("mint_one").unwrap(),
                            vec![],
                            args,
                        );
                    }
                }
            }
            builder.finish()
        };
        TestTransactionBuilder::new(
            account.sender,
            account.gas_objects[0],
            DEFAULT_VALIDATOR_GAS_PRICE,
        )
        .programmable(pt)
        .build_and_sign(account.keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Programmable Move Transaction Generator"
    }
}
