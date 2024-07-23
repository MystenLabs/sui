// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::stream::FuturesUnordered;
use sui_types::committee::EpochId;
use sui_types::utils::load_test_vectors;

use std::sync::Arc;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::{get_account_key_pair as get_random_ed25519_key_pair, AccountKeyPair as Ed25519KeyPair, SuiKeyPair};
use sui_types::object::Object;

use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;

pub enum MockKeyPair {
    Ed25519(Ed25519KeyPair),
    ZkLogin(ZkLoginEphKeyPair),
}

#[derive(Debug)]
pub struct ZkLoginEphKeyPair {
    pub private: SuiKeyPair,
    pub public: ZkLoginAuxInputs
}

pub fn copy_zk_login_eph_key_pair(kp: &ZkLoginEphKeyPair) -> ZkLoginEphKeyPair {
    ZkLoginEphKeyPair {
        private: kp.private.copy(),
        public: kp.public.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ZkLoginAuxInputs { // ZK proof and remaining public inputs
    pub zkp_details: ZkLoginInputs,
    pub max_epoch: EpochId,
}

#[derive(Clone)]
pub struct Account {
    pub sender: SuiAddress,
    pub keypair: Arc<MockKeyPair>,
    pub gas_objects: Arc<Vec<ObjectRef>>,
}

pub fn get_test_zklogin_key_pair() -> (SuiAddress, ZkLoginEphKeyPair) {
    let (kp, pk, inputs) = 
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    (
        pk.into(),
        ZkLoginEphKeyPair {
            private: kp.copy(),
            public: ZkLoginAuxInputs {
                zkp_details: inputs.clone(),
                max_epoch: 2,
            }
        }
    )
}

/// Generate \num_accounts accounts and for each account generate \gas_object_num_per_account gas objects.
/// Return all accounts along with a flattened list of all gas objects as genesis objects.
pub async fn batch_create_account_and_gas(
    num_accounts: u64,
    gas_object_num_per_account: u64,
    use_zklogin: bool,
) -> (Vec<(SuiAddress, Account)>, Vec<Object>) {
    // Uses the same zklogin key pair for all accounts
    let (s, k) = get_test_zklogin_key_pair();
    let mut v = Vec::new();
    for _ in 0..num_accounts {
        v.push((s, MockKeyPair::ZkLogin(copy_zk_login_eph_key_pair(&k))))
    }

    let tasks: FuturesUnordered<_> = (0..num_accounts)
        .map(|_| {
            let x = v.pop().unwrap();
            tokio::spawn(async move {
                let (sender, keypair) = 
                    if use_zklogin { 
                        x
                    } else {
                        let (s, k) = get_random_ed25519_key_pair();
                        (s, MockKeyPair::Ed25519(k))
                    };
                let objects = (0..gas_object_num_per_account)
                    .map(|_| Object::with_owner_for_testing(sender))
                    .collect::<Vec<_>>();
                (sender, keypair, objects)
            })
        })
        .collect();
    let mut accounts = Vec::new();
    let mut genesis_gas_objects = vec![];
    for task in tasks {
        let (sender, keypair, gas_objects) = task.await.unwrap();
        let gas_object_refs: Vec<_> = gas_objects
            .iter()
            .map(|o| o.compute_object_reference())
            .collect();
        accounts.push((
            sender,
            Account {
                sender,
                keypair: Arc::new(keypair),
                gas_objects: Arc::new(gas_object_refs),
            },
        ));
        genesis_gas_objects.extend(gas_objects);
    }
    (accounts, genesis_gas_objects)
}
