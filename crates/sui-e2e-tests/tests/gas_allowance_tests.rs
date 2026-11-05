// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use std::path::PathBuf;
use sui_macros::sim_test;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_simulator::has_mainnet_protocol_config_override;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::UserInputError,
    execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus},
    gas::SuiGasStatus,
    gas_allowance::{GasAllowance, StorageRebate},
    gas_coin::GasCoin,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    sui_system_state::SuiSystemStateTrait,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{
        Argument, Command, GasData, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
        Transaction, TransactionData, TransactionDataV1, TransactionExpiration, TransactionKind,
    },
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

const BUDGET: u64 = 20_000_000;
const FUND: u64 = 500_000_000;

async fn issue(env: &mut TestEnv, spender: SuiAddress, amount: u64) -> (ObjectID, SequenceNumber) {
    let funder = env.get_sender(0);
    let mut pt = ProgrammableTransactionBuilder::new();
    let amount = pt.pure(amount).unwrap();
    let payment = pt.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let spender = pt.pure(spender).unwrap();
    pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![],
        vec![payment, spender],
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![env.get_gas_for_sender(funder)[0]],
        pt.finish(),
        BUDGET,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    effects
        .created()
        .into_iter()
        .find_map(|(object, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((object.0, initial_shared_version)),
            _ => None,
        })
        .unwrap()
}

fn allowance_input(id: ObjectID, version: SequenceNumber) -> ObjectArg {
    ObjectArg::SharedObject {
        id,
        initial_shared_version: version,
        mutability: SharedObjectMutability::Mutable,
    }
}

fn transaction(
    env: &TestEnv,
    sender: SuiAddress,
    allowance: (ObjectID, SequenceNumber),
    nonce: u32,
    budget: u64,
    mut pt: ProgrammableTransaction,
) -> TransactionData {
    pt.inputs
        .push(sui_types::transaction::CallArg::Object(allowance_input(
            allowance.0,
            allowance.1,
        )));
    TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: allowance.0.into(),
            price: env.rgp,
            budget,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: env.chain_id,
            nonce,
        },
    })
}

async fn state(env: &TestEnv, id: ObjectID) -> GasAllowance {
    GasAllowance::from_object(
        &env.cluster
            .get_object_from_fullnode_store(&id)
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn redeem_storage_claim(
    env: &mut TestEnv,
    funder: SuiAddress,
    settlement: &TransactionEffects,
) {
    let claim_id = StorageRebate::claim_id(funder, settlement.transaction_digest());
    let claim = env
        .cluster
        .get_object_from_fullnode_store(&claim_id)
        .await
        .expect("funder-owned storage claim");
    assert_eq!(claim.owner, Owner::AddressOwner(funder));
    assert!(claim.storage_rebate > 0);
    let gas = env.get_gas_for_sender(funder)[0];
    let gas_before = env
        .cluster
        .get_object_from_fullnode_store(&gas.0)
        .await
        .unwrap();
    let mut pt = ProgrammableTransactionBuilder::new();
    let input = pt
        .obj(ObjectArg::ImmOrOwnedObject(
            claim.compute_object_reference(),
        ))
        .unwrap();
    pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("claim_storage_rebate").unwrap(),
        vec![],
        vec![input],
    );
    let (_, effects) = env
        .exec_tx_directly(TransactionData::new_programmable(
            funder,
            vec![gas],
            pt.finish(),
            BUDGET,
            env.rgp,
        ))
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let gas_after = env
        .cluster
        .get_object_from_fullnode_store(&gas.0)
        .await
        .unwrap();
    assert_eq!(
        i128::from(
            sui_types::gas_coin::GasCoin::try_from(&gas_before)
                .unwrap()
                .value()
        ) - i128::from(
            sui_types::gas_coin::GasCoin::try_from(&gas_after)
                .unwrap()
                .value()
        ),
        i128::from(effects.gas_cost_summary().net_gas_usage()),
    );
    assert!(
        env.cluster
            .get_object_from_fullnode_store(&claim_id)
            .await
            .is_none(),
        "funder claim is consumed"
    );
}

async fn execute(env: &TestEnv, tx: TransactionData, key: &AccountKeyPair) -> TransactionEffects {
    let signed = Transaction::from_data_and_signer(tx, vec![key]);
    let (digest, effects) = env
        .cluster
        .execute_transaction_directly(&signed)
        .await
        .unwrap();
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    effects
}

async fn package(env: &mut TestEnv) -> ObjectID {
    env.setup_test_package(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/move_test_code"))
        .await
}

fn create_storage(pkg: ObjectID) -> ProgrammableTransaction {
    let mut pt = ProgrammableTransactionBuilder::new();
    let value = pt.pure(1u64).unwrap();
    let data = pt.pure(vec![7u8; 16_000]).unwrap();
    pt.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data],
    );
    pt.finish()
}

#[sim_test]
async fn unfunded_spender_repeats_without_funder_signatures_and_cannot_replay() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let allowance = issue(&mut env, spender, FUND).await;
    assert!(
        env.cluster
            .wallet
            .gas_objects(spender)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(env.get_sui_balance_ab(spender), 0);
    for nonce in 0..2 {
        let before = state(&env, allowance.0).await.balance.value();
        let tx = transaction(
            &env,
            spender,
            allowance,
            nonce,
            BUDGET,
            ProgrammableTransactionBuilder::new().finish(),
        );
        let simulation = env
            .cluster
            .grpc_client()
            .simulate_transaction(&tx, true, nonce == 1)
            .await
            .unwrap();
        assert!(simulation.transaction.effects.status().is_ok());
        assert!(simulation.transaction.effects.gas_object().is_none());
        let signed = Transaction::from_data_and_signer(tx, vec![&key]);
        // Exercise the public RPC path with only the spender's signature.
        env.cluster.execute_transaction(signed.clone()).await;
        let (digest, effects) = env
            .cluster
            .execute_transaction_directly(&signed)
            .await
            .unwrap();
        env.cluster.wait_for_tx_settlement(&[digest]).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        assert!(effects.gas_object().is_none());
        let after = state(&env, allowance.0).await.balance.value();
        assert!(after < before && before - after <= BUDGET);
        assert_eq!(
            i128::from(before - after),
            i128::from(effects.gas_cost_summary().net_gas_usage()),
        );
        assert_eq!(env.get_sui_balance_ab(env.get_sender(0)), 0);
        let (_, replay) = env
            .cluster
            .execute_transaction_directly(&signed)
            .await
            .unwrap();
        assert_eq!(replay, effects);
        assert_eq!(state(&env, allowance.0).await.balance.value(), after);
        assert_eq!(env.get_sui_balance_ab(spender), 0);
        assert!(
            env.cluster
                .wallet
                .gas_objects(spender)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[sim_test]
async fn wrong_spender_exhaustion_and_gas_extraction_are_rejected() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let (attacker, attacker_key): (_, AccountKeyPair) = get_key_pair();
    let allowance = issue(&mut env, spender, BUDGET).await;
    let wrong = transaction(
        &env,
        attacker,
        allowance,
        0,
        BUDGET,
        ProgrammableTransactionBuilder::new().finish(),
    );
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(
                wrong,
                vec![&attacker_key]
            ))
            .await
            .is_err()
    );
    let mut pt = ProgrammableTransactionBuilder::new();
    pt.transfer_arg(spender, Argument::GasCoin);
    let extraction = transaction(&env, spender, allowance, 1, BUDGET, pt.finish());
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(
                extraction,
                vec![&key]
            ))
            .await
            .is_err()
    );
    let funder = env.get_sender(0);
    env.transfer_sui_to_address_balance(funder, funder, BUDGET)
        .await;
    let mut withdrawal = ProgrammableTransactionBuilder::new();
    withdrawal
        .input(sui_types::transaction::CallArg::FundsWithdrawal(
            sui_types::transaction::FundsWithdrawalArg::balance_from_allowance(
                1,
                sui_types::gas_coin::GAS::type_tag(),
                funder,
                allowance.0,
            ),
        ))
        .unwrap();
    let withdrawal = transaction(&env, spender, allowance, 4, BUDGET, withdrawal.finish());
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(
                withdrawal,
                vec![&key]
            ))
            .await
            .is_err()
    );
    assert_eq!(state(&env, allowance.0).await.balance.value(), BUDGET);
    let first = transaction(
        &env,
        spender,
        allowance,
        2,
        BUDGET,
        ProgrammableTransactionBuilder::new().finish(),
    );
    assert!(execute(&env, first, &key).await.status().is_ok());
    let exhausted = transaction(
        &env,
        spender,
        allowance,
        3,
        BUDGET,
        ProgrammableTransactionBuilder::new().finish(),
    );
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(exhausted, vec![&key]))
            .await
            .is_err()
    );
}

#[sim_test]
async fn an_unrelated_shared_object_cannot_authorize_allowance_gas() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let funder = env.get_sender(0);
    let mut pt = ProgrammableTransactionBuilder::new();
    pt.programmable_move_call(
        pkg,
        Identifier::new("shared_objects_version").unwrap(),
        Identifier::new("create_shared_counter").unwrap(),
        vec![],
        vec![],
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![env.get_gas_for_sender(funder)[0]],
        pt.finish(),
        BUDGET,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    let shared = effects
        .created()
        .into_iter()
        .find_map(|(object, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((object.0, initial_shared_version)),
            _ => None,
        })
        .unwrap();
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let tx = transaction(
        &env,
        spender,
        shared,
        0,
        BUDGET,
        ProgrammableTransactionBuilder::new().finish(),
    );
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(tx, vec![&key]))
            .await
            .is_err()
    );
}

#[sim_test]
async fn failures_and_storage_oog_charge_only_the_escrow() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let allowance = issue(&mut env, spender, FUND).await;
    let mut abort = ProgrammableTransactionBuilder::new();
    let yes = abort.pure(true).unwrap();
    abort.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("abort_with_computation").unwrap(),
        vec![],
        vec![yes],
    );
    for (nonce, pt) in [abort.finish(), create_storage(pkg)]
        .into_iter()
        .enumerate()
    {
        let before = state(&env, allowance.0).await.balance.value();
        let funder = env.get_sender(0);
        let refunds_before = env.get_sui_balance_ab(funder);
        let tx = transaction(&env, spender, allowance, nonce as u32, BUDGET, pt);
        let effects = execute(&env, tx, &key).await;
        assert!(effects.status().is_err(), "{:?}", effects.status());
        let after = state(&env, allowance.0).await.balance.value();
        assert!(after < before && before - after <= BUDGET);
        let refunds_after = env.get_sui_balance_ab(funder);
        assert_eq!(
            i128::from(before) + i128::from(refunds_before)
                - i128::from(after)
                - i128::from(refunds_after),
            i128::from(effects.gas_cost_summary().net_gas_usage()),
        );
        assert_eq!(env.get_sui_balance_ab(spender), 0);
        assert!(
            env.cluster
                .wallet
                .gas_objects(spender)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[sim_test]
async fn ordinary_payer_abort_preserves_allowance_storage_provenance() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Cover budgets both above and below the protected replacement deposit with each ordinary
    // payment mode. The separate ordinary object ensures storage metering still occurs.
    for gas_budget in [200_000_000, BUDGET] {
        for pay_from_address_balance in [true, false] {
            let mut env = TestEnvBuilder::new().build().await;
            let pkg = package(&mut env).await;
            let spender = env.get_sender(1);
            let funder = env.get_sender(0);
            let allowance = issue(&mut env, spender, FUND).await;
            let (_, created) = env
                .exec_tx_directly(transaction(
                    &env,
                    spender,
                    allowance,
                    0,
                    200_000_000,
                    create_storage(pkg),
                ))
                .await
                .unwrap();
            assert!(created.status().is_ok(), "{:?}", created.status());
            let object = created
                .created()
                .into_iter()
                .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
                .expect("allowance-funded object")
                .0;
            let protected_deposit = env
                .cluster
                .get_object_from_fullnode_store(&object.0)
                .await
                .expect("allowance-funded object remains readable")
                .storage_rebate;

            let mut create_ordinary = ProgrammableTransactionBuilder::new();
            let value = create_ordinary.pure(0u64).unwrap();
            create_ordinary.programmable_move_call(
                pkg,
                Identifier::new("gas_test").unwrap(),
                Identifier::new("create_object_with_storage").unwrap(),
                vec![],
                vec![value],
            );
            let (_, ordinary_created) = env
                .exec_tx_directly(TransactionData::new_programmable(
                    spender,
                    vec![env.get_gas_for_sender(spender)[0]],
                    create_ordinary.finish(),
                    200_000_000,
                    env.rgp,
                ))
                .await
                .unwrap();
            assert!(
                ordinary_created.status().is_ok(),
                "{:?}",
                ordinary_created.status()
            );
            let ordinary_object = ordinary_created
                .created()
                .into_iter()
                .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
                .expect("ordinary mutable object")
                .0;
            env.update_all_gas().await;

            let funder_before_abort = env.get_sui_balance_ab(funder);
            let payer_before_abort = if pay_from_address_balance {
                env.fund_one_address_balance(spender, FUND).await;
                Some(env.get_sui_balance_ab(spender))
            } else {
                None
            };
            let mut abort = ProgrammableTransactionBuilder::new();
            let protected = abort.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
            let ordinary = abort
                .obj(ObjectArg::ImmOrOwnedObject(ordinary_object))
                .unwrap();
            abort.programmable_move_call(
                pkg,
                Identifier::new("gas_test").unwrap(),
                Identifier::new("mutate_two_and_abort").unwrap(),
                vec![],
                vec![protected, ordinary],
            );
            let aborted_tx = TransactionData::new_programmable(
                spender,
                if pay_from_address_balance {
                    vec![]
                } else {
                    vec![env.get_gas_for_sender(spender)[0]]
                },
                abort.finish(),
                gas_budget,
                env.rgp,
            );
            let (_, aborted) = env.exec_tx_directly(aborted_tx).await.unwrap();
            match aborted.status() {
                ExecutionStatus::Failure(ExecutionFailure {
                    error: ExecutionFailureStatus::MoveAbort(location, 42),
                    ..
                }) => assert_eq!(
                    location.function_name.as_deref(),
                    Some("mutate_two_and_abort")
                ),
                other => panic!("expected original Move abort, got {other:?}"),
            }
            assert!(
                aborted.gas_cost_summary().computation_cost > 0
                    && aborted.gas_cost_summary().storage_cost > 0,
                "the ordinary payer must be charged for computation and unprotected storage"
            );
            assert_eq!(env.get_sui_balance_ab(funder), funder_before_abort);
            if let Some(payer_before_abort) = payer_before_abort {
                assert!(
                    env.get_sui_balance_ab(spender) < payer_before_abort,
                    "the address-balance payer still pays computation and input reads"
                );
                assert!(payer_before_abort - env.get_sui_balance_ab(spender) < 100_000_000);
            } else {
                assert!(aborted.gas_object().is_some(), "the coin payer was charged");
            }
            let object = aborted
                .mutated()
                .into_iter()
                .find(|(reference, _)| reference.0 == object.0)
                .expect("aborted protected input remains version-bumped")
                .0;
            assert!(
                aborted
                    .mutated()
                    .into_iter()
                    .any(|(reference, _)| reference.0 == ordinary_object.0),
                "the ordinary mutable input remains version-bumped"
            );
            assert_eq!(
                env.cluster
                    .get_object_from_fullnode_store(&object.0)
                    .await
                    .expect("recovered object")
                    .storage_rebate,
                protected_deposit,
                "recovery retains the protected deposit"
            );
            assert!(
                env.cluster
                    .get_object_from_fullnode_store(&StorageRebate::child_id(object.0))
                    .await
                    .is_some(),
                "the protected object's marker survives recovery"
            );

            env.update_all_gas().await;
            let mut deletion = ProgrammableTransactionBuilder::new();
            let input = deletion.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
            deletion.programmable_move_call(
                pkg,
                Identifier::new("gas_test").unwrap(),
                Identifier::new("delete_object").unwrap(),
                vec![],
                vec![input],
            );
            let deleted_tx = TransactionData::new_programmable(
                spender,
                if pay_from_address_balance {
                    vec![]
                } else {
                    vec![env.get_gas_for_sender(spender)[0]]
                },
                deletion.finish(),
                BUDGET,
                env.rgp,
            );
            let (_, deleted) = env.exec_tx_directly(deleted_tx).await.unwrap();
            assert!(deleted.status().is_ok(), "{:?}", deleted.status());
            redeem_storage_claim(&mut env, funder, &deleted).await;
        }
    }
}

#[sim_test]
async fn insufficient_provenance_prepayment_rejects_before_smashing_protected_secondary() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let spender = env.get_sender(1);
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let funding = env
        .tx_builder_with_gas(funder, env.get_gas_for_sender(funder)[0])
        .transfer_sui(Some(FUND), spender)
        .build();
    let (_, funding_effects) = env.exec_tx_directly(funding).await.unwrap();
    assert!(funding_effects.status().is_ok());
    let source_coin = funding_effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("spender funding coin")
        .0;
    let mut mint_coin = ProgrammableTransactionBuilder::new();
    let source_coin = mint_coin
        .obj(ObjectArg::ImmOrOwnedObject(source_coin))
        .unwrap();
    let amount = mint_coin.pure(FUND).unwrap();
    let coin = mint_coin.command(Command::SplitCoins(source_coin, vec![amount]));
    mint_coin.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("destroy_zero").unwrap(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![source_coin],
    );
    mint_coin.transfer_arg(spender, coin);
    let (_, created) = env
        .exec_tx_directly(transaction(
            &env,
            spender,
            allowance,
            0,
            200_000_000,
            mint_coin.finish(),
        ))
        .await
        .unwrap();
    assert!(created.status().is_ok(), "{:?}", created.status());
    let protected_secondary = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("allowance-funded secondary gas coin")
        .0;

    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let base_budget = config
        .base_tx_cost_fixed()
        .checked_mul(env.rgp)
        .expect("test gas price is bounded");
    let input_count = usize::try_from(config.max_arguments()).unwrap();
    let lookup_budget = SuiGasStatus::minimum_gas_budget_for_storage_read(
        StorageRebate::input_read_bytes(input_count, &config),
        env.rgp,
        env.rgp,
        &config,
    )
    .unwrap();
    let budget = lookup_budget.checked_sub(1).unwrap();
    assert!(budget >= base_budget);

    let funding = env
        .tx_builder_with_gas(funder, env.get_gas_for_sender(funder)[0])
        .transfer_sui(Some(2_000_000_000), spender)
        .build();
    let (_, funding_effects) = env.exec_tx_directly(funding).await.unwrap();
    let source_coin = funding_effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("spender coin for small payments")
        .0;
    let mut split = ProgrammableTransactionBuilder::new();
    let amounts = (0..input_count - 1)
        .map(|_| split.pure(1u64).unwrap())
        .collect();
    let Argument::Result(split_index) =
        split.command(Command::SplitCoins(Argument::GasCoin, amounts))
    else {
        panic!("SplitCoins should return Argument::Result");
    };
    let recipient = split.pure(spender).unwrap();
    split.command(Command::TransferObjects(
        (0..input_count - 1)
            .map(|index| Argument::NestedResult(split_index, u16::try_from(index).unwrap()))
            .collect(),
        recipient,
    ));
    let (_, split_effects) = env
        .exec_tx_directly(TransactionData::new_programmable(
            spender,
            vec![source_coin],
            split.finish(),
            1_000_000_000,
            env.rgp,
        ))
        .await
        .unwrap();
    assert!(
        split_effects.status().is_ok(),
        "{:?}",
        split_effects.status()
    );
    let tiny_coins: Vec<_> = split_effects
        .created()
        .into_iter()
        .filter(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .map(|(reference, _)| reference)
        .collect();
    assert_eq!(tiny_coins.len(), input_count - 1);
    let marker_id = StorageRebate::child_id(protected_secondary.0);
    let protected_before = env
        .cluster
        .get_object_from_fullnode_store(&protected_secondary.0)
        .await
        .expect("protected secondary before rejected payment");
    let marker_before = env
        .cluster
        .get_object_from_fullnode_store(&marker_id)
        .await
        .expect("protected secondary marker before rejected payment");
    assert!(
        GasCoin::try_from(&protected_before).unwrap().value() >= budget,
        "the protected secondary plus the tiny primary coins cover the rejected budget"
    );
    let payments = vec![tiny_coins[0], protected_secondary];
    let mut inputs = ProgrammableTransactionBuilder::new();
    for coin in tiny_coins.into_iter().skip(1) {
        inputs.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    }
    assert!(
        GasCoin::try_from(
            &env.cluster
                .get_object_from_fullnode_store(&payments[0].0)
                .await
                .expect("tiny primary gas coin"),
        )
        .unwrap()
        .value()
            < budget
    );

    let rejected =
        TransactionData::new_programmable(spender, payments, inputs.finish(), budget, env.rgp);
    let signed = env.cluster.wallet.sign_transaction(&rejected).await;
    let error = env
        .cluster
        .execute_transaction_directly(&signed)
        .await
        .expect_err("admission must reject unpaid provenance lookups");
    assert_eq!(
        UserInputError::try_from(error).unwrap(),
        UserInputError::GasBudgetTooLow {
            gas_budget: budget,
            min_budget: lookup_budget,
        }
    );
    assert_eq!(
        env.cluster
            .get_object_from_fullnode_store(&protected_secondary.0)
            .await
            .expect("protected secondary after rejected payment")
            .compute_object_reference(),
        protected_before.compute_object_reference()
    );
    assert_eq!(
        env.cluster
            .get_object_from_fullnode_store(&marker_id)
            .await
            .expect("protected secondary marker after rejected payment")
            .compute_object_reference(),
        marker_before.compute_object_reference()
    );
}

#[sim_test]
async fn abort_smashing_protected_secondary_gas_coin_creates_funder_claim() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let spender = env.get_sender(1);
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let funding = env
        .tx_builder_with_gas(funder, env.get_gas_for_sender(funder)[0])
        .transfer_sui(Some(FUND), spender)
        .build();
    let (_, funding_effects) = env.exec_tx_directly(funding).await.unwrap();
    assert!(funding_effects.status().is_ok());
    let source_coin = funding_effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;
    let mut mint_coin = ProgrammableTransactionBuilder::new();
    let source_coin = mint_coin
        .obj(ObjectArg::ImmOrOwnedObject(source_coin))
        .unwrap();
    let amount = mint_coin.pure(FUND).unwrap();
    let coin = mint_coin.command(Command::SplitCoins(source_coin, vec![amount]));
    mint_coin.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("destroy_zero").unwrap(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![source_coin],
    );
    mint_coin.transfer_arg(spender, coin);
    let (_, created) = env
        .exec_tx_directly(transaction(
            &env,
            spender,
            allowance,
            0,
            200_000_000,
            mint_coin.finish(),
        ))
        .await
        .unwrap();
    assert!(created.status().is_ok(), "{:?}", created.status());
    let protected_secondary = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("allowance-funded secondary gas coin")
        .0;
    env.update_all_gas().await;
    let primary = env
        .get_gas_for_sender(spender)
        .into_iter()
        .find(|reference| reference.0 != protected_secondary.0)
        .expect("unprotected primary gas coin");
    let primary_value = GasCoin::try_from(
        &env.cluster
            .get_object_from_fullnode_store(&primary.0)
            .await
            .expect("primary gas coin"),
    )
    .unwrap()
    .value();
    let protected_secondary_value = GasCoin::try_from(
        &env.cluster
            .get_object_from_fullnode_store(&protected_secondary.0)
            .await
            .expect("protected secondary gas coin"),
    )
    .unwrap()
    .value();

    let mut abort = ProgrammableTransactionBuilder::new();
    let should_abort = abort.pure(true).unwrap();
    abort.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("abort_with_computation").unwrap(),
        vec![],
        vec![should_abort],
    );
    let tx = TransactionData::new_programmable(
        spender,
        vec![primary, protected_secondary],
        abort.finish(),
        200_000_000,
        env.rgp,
    );
    let funder_balance_before = env.get_sui_balance_ab(funder);
    let signed = env.cluster.wallet.sign_transaction(&tx).await;
    let (effects, events) = env
        .cluster
        .execute_transaction_return_raw_effects(signed)
        .await
        .unwrap();
    env.cluster
        .wait_for_tx_settlement(&[*effects.transaction_digest()])
        .await;
    assert!(
        events.data.is_empty(),
        "claim settlement must not emit user events after a Move abort"
    );
    assert_eq!(
        env.get_sui_balance_ab(funder),
        funder_balance_before,
        "the raw refund remains in the funder-owned claim until redemption"
    );
    match effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, 42),
            ..
        }) => assert_eq!(
            location.function_name.as_deref(),
            Some("abort_with_computation")
        ),
        other => panic!("expected original Move abort, got {other:?}"),
    }
    assert!(
        effects.gas_cost_summary().computation_cost > 0,
        "the unprotected primary coin paid for the failed transaction"
    );
    let gas_object = effects.gas_object().expect("primary gas object");
    assert_eq!(
        gas_object.0.0, primary.0,
        "the first payment coin remains the smash target"
    );
    let payer_fee = u64::try_from(effects.gas_cost_summary().net_gas_usage())
        .expect("failed transaction has a positive payer fee");
    let primary_value_after = GasCoin::try_from(
        &env.cluster
            .get_object_from_fullnode_store(&primary.0)
            .await
            .expect("smashed primary gas coin"),
    )
    .unwrap()
    .value();
    assert_eq!(
        primary_value + protected_secondary_value - primary_value_after,
        payer_fee,
        "effects report the primary payer's actual fee"
    );
    redeem_storage_claim(&mut env, funder, &effects).await;
}

#[sim_test]
async fn same_allowance_recycles_large_object_storage_and_preserves_provenance() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let tx = transaction(
        &env,
        spender,
        allowance,
        0,
        200_000_000,
        create_storage(pkg),
    );
    let created = execute(&env, tx, &key).await;
    assert!(created.status().is_ok(), "{:?}", created.status());
    let mut object = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;

    for nonce in 1..=2 {
        let allowance_before = state(&env, allowance.0).await.balance.value();
        let funder_before = env.get_sui_balance_ab(funder);
        let mut transfer = ProgrammableTransactionBuilder::new();
        let input = transfer.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
        let recipient = transfer.pure(spender).unwrap();
        transfer.programmable_move_call(
            pkg,
            Identifier::new("gas_test").unwrap(),
            Identifier::new("transfer_object").unwrap(),
            vec![],
            vec![input, recipient],
        );
        let effects = execute(
            &env,
            transaction(&env, spender, allowance, nonce, BUDGET, transfer.finish()),
            &key,
        )
        .await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        let allowance_after = state(&env, allowance.0).await.balance.value();
        assert_eq!(
            i128::from(allowance_before - allowance_after),
            i128::from(effects.gas_cost_summary().net_gas_usage()),
        );
        assert_eq!(env.get_sui_balance_ab(funder), funder_before);
        object = effects
            .mutated()
            .into_iter()
            .find(|(reference, _)| reference.0 == object.0)
            .unwrap()
            .0;
    }

    env.transfer_sui_to_address_balance(funder, spender, BUDGET)
        .await;
    let mut deletion = ProgrammableTransactionBuilder::new();
    let input = deletion.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
    deletion.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![input],
    );
    let tx = TransactionData::new_with_gas_data_and_expiration(
        TransactionKind::ProgrammableTransaction(deletion.finish()),
        spender,
        GasData {
            payment: vec![],
            owner: spender,
            price: env.rgp,
            budget: BUDGET,
        },
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: env.chain_id,
            nonce: 3,
        },
    );
    let effects = execute(&env, tx, &key).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    redeem_storage_claim(&mut env, funder, &effects).await;
}

#[sim_test]
async fn delayed_storage_refunds_belong_to_the_funder_after_revocation() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let tx = transaction(
        &env,
        spender,
        allowance,
        0,
        200_000_000,
        create_storage(pkg),
    );
    let created = execute(&env, tx, &key).await;
    assert!(created.status().is_ok(), "{:?}", created.status());
    let object = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;
    let rebate = env
        .cluster
        .get_object_from_fullnode_store(&object.0)
        .await
        .unwrap()
        .storage_rebate;
    assert!(rebate > 100_000_000);

    // An object-owned marker is not an address-owned claim. The spender cannot bypass
    // attribution by invoking the claim entry point on the marker directly.
    let marker = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::ObjectOwner(object.0.into()))
        .unwrap()
        .0;
    let mut claim = ProgrammableTransactionBuilder::new();
    let input = claim.obj(ObjectArg::ImmOrOwnedObject(marker)).unwrap();
    claim.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("claim_storage_rebate").unwrap(),
        vec![],
        vec![input],
    );
    let tx = transaction(&env, spender, allowance, 2, BUDGET, claim.finish());
    assert!(
        env.cluster
            .execute_transaction_directly(&Transaction::from_data_and_signer(tx, vec![&key]))
            .await
            .is_err()
    );

    let mut revoke = ProgrammableTransactionBuilder::new();
    let input = revoke
        .obj(allowance_input(allowance.0, allowance.1))
        .unwrap();
    revoke.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("revoke").unwrap(),
        vec![],
        vec![input],
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![env.get_gas_for_sender(funder)[0]],
        revoke.finish(),
        BUDGET,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(state(&env, allowance.0).await.revoked);
    assert_eq!(state(&env, allowance.0).await.balance.value(), 0);

    // The spender acquires independent gas only for the attempted refund extraction.
    env.transfer_sui_to_address_balance(funder, spender, BUDGET)
        .await;
    let spender_before = env.get_sui_balance_ab(spender);
    let mut deletion = ProgrammableTransactionBuilder::new();
    let input = deletion.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
    deletion.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![input],
    );
    let mut tx = transaction(&env, spender, allowance, 1, BUDGET, deletion.finish());
    let TransactionData::V1(data) = &mut tx;
    let TransactionKind::ProgrammableTransaction(pt) = &mut data.kind else {
        unreachable!()
    };
    pt.inputs.pop();
    data.gas_data.owner = spender;
    let effects = execute(&env, tx, &key).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    redeem_storage_claim(&mut env, funder, &effects).await;
    assert!(env.get_sui_balance_ab(spender) < spender_before);
    assert!(
        env.cluster
            .get_object_from_fullnode_store(&object.0)
            .await
            .is_none()
    );
}

#[sim_test]
async fn concurrent_admission_cannot_overdraw_and_revocation_wins_in_order() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let allowance = issue(&mut env, spender, BUDGET).await;
    let signed: Vec<_> = (0..2)
        .map(|nonce| {
            Transaction::from_data_and_signer(
                transaction(
                    &env,
                    spender,
                    allowance,
                    nonce,
                    BUDGET,
                    ProgrammableTransactionBuilder::new().finish(),
                ),
                vec![&key],
            )
        })
        .collect();
    let results = env
        .cluster
        .execute_signed_txns_in_soft_bundle(&signed)
        .await
        .unwrap();
    env.cluster
        .wait_for_tx_settlement(
            &results
                .iter()
                .map(|(digest, _)| *digest)
                .collect::<Vec<_>>(),
        )
        .await;
    assert!(results[0].1.status().is_ok());
    assert!(results[1].1.status().is_err());
    assert_eq!(results[1].1.gas_cost_summary().gas_used(), 0);
    assert!(state(&env, allowance.0).await.balance.value() < BUDGET);

    let allowance = issue(&mut env, spender, FUND).await;
    let funder = env.get_sender(0);
    let mut revoke = ProgrammableTransactionBuilder::new();
    let input = revoke
        .obj(allowance_input(allowance.0, allowance.1))
        .unwrap();
    revoke.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("revoke").unwrap(),
        vec![],
        vec![input],
    );
    let revoke = TransactionData::new_programmable(
        funder,
        vec![env.get_gas_for_sender(funder)[0]],
        revoke.finish(),
        BUDGET,
        env.rgp,
    );
    let revoke = env.cluster.wallet.sign_transaction(&revoke).await;
    let spend = Transaction::from_data_and_signer(
        transaction(
            &env,
            spender,
            allowance,
            2,
            BUDGET,
            ProgrammableTransactionBuilder::new().finish(),
        ),
        vec![&key],
    );
    let results = env
        .cluster
        .execute_signed_txns_in_soft_bundle(&[revoke, spend])
        .await
        .unwrap();
    env.cluster
        .wait_for_tx_settlement(
            &results
                .iter()
                .map(|(digest, _)| *digest)
                .collect::<Vec<_>>(),
        )
        .await;
    assert!(results[0].1.status().is_ok());
    assert!(results[1].1.status().is_err());
    assert_eq!(results[1].1.gas_cost_summary().gas_used(), 0);
    assert!(state(&env, allowance.0).await.revoked);
    assert_eq!(state(&env, allowance.0).await.balance.value(), 0);
}

#[sim_test]
async fn recycling_existing_storage_does_not_charge_the_next_payer_for_its_deposit() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let spender = env.get_sender(1);
    let next_owner = env.get_sender(2);
    let allowance = issue(&mut env, spender, FUND).await;
    let tx = TransactionData::new_programmable(
        spender,
        vec![env.get_gas_for_sender(spender)[0]],
        create_storage(pkg),
        200_000_000,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let object = effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;

    // The allowance recycles an existing deposit; it does not buy new storage.
    let mut transfer = ProgrammableTransactionBuilder::new();
    let input = transfer.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
    let recipient = transfer.pure(next_owner).unwrap();
    transfer.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("transfer_object").unwrap(),
        vec![],
        vec![input, recipient],
    );
    let tx = transaction(&env, spender, allowance, 0, BUDGET, transfer.finish());
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let mut object = effects
        .mutated()
        .into_iter()
        .find(|(reference, _)| reference.0 == object.0)
        .unwrap()
        .0;
    for _ in 0..2 {
        let mut pt = ProgrammableTransactionBuilder::new();
        let input = pt.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
        let recipient = pt.pure(next_owner).unwrap();
        pt.programmable_move_call(
            pkg,
            Identifier::new("gas_test").unwrap(),
            Identifier::new("transfer_object").unwrap(),
            vec![],
            vec![input, recipient],
        );
        let tx = TransactionData::new_programmable(
            next_owner,
            vec![env.get_gas_for_sender(next_owner)[0]],
            pt.finish(),
            BUDGET,
            env.rgp,
        );
        let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        object = effects
            .mutated()
            .into_iter()
            .find(|(reference, _)| reference.0 == object.0)
            .unwrap()
            .0;
    }
}

#[sim_test]
async fn automatic_budget_covers_storage_refunded_to_another_payer() {
    use sui_rpc::proto::sui::rpc::v2::{
        Argument as RpcArgument, Command as RpcCommand, GasPayment, Input, MoveCall,
        ProgrammableTransaction as RpcPtb, SimulateTransactionRequest,
        Transaction as RpcTransaction, TransactionKind as RpcTransactionKind,
        transaction_execution_service_client::TransactionExecutionServiceClient,
    };
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let spender = env.get_sender(1);
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let tx = transaction(
        &env,
        spender,
        allowance,
        0,
        200_000_000,
        create_storage(pkg),
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let object = effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;
    env.fund_one_address_balance(spender, FUND).await;
    let spender_before = env.get_sui_balance_ab(spender);

    let mut input = Input::default();
    input.object_id = Some(object.0.to_canonical_string(true));
    let mut recipient = Input::default();
    recipient.literal = Some(Box::new(spender.to_string().into()));
    let mut transfer = MoveCall::default();
    transfer.package = Some(pkg.to_canonical_string(true));
    transfer.module = Some("gas_test".to_owned());
    transfer.function = Some("transfer_object".to_owned());
    transfer.arguments = vec![RpcArgument::new_input(0), RpcArgument::new_input(1)];
    let mut pt = RpcPtb::default();
    pt.inputs = vec![input, recipient];
    pt.commands = vec![RpcCommand::from(transfer)];
    let mut unresolved = RpcTransaction::default();
    unresolved.kind = Some(RpcTransactionKind::from(pt));
    unresolved.sender = Some(spender.to_string());
    let mut gas = GasPayment::default();
    gas.owner = Some(spender.to_string());
    unresolved.gas_payment = Some(gas);
    let mut client = TransactionExecutionServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let response = client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let resolved: TransactionData = response
        .transaction
        .as_ref()
        .unwrap()
        .transaction
        .as_ref()
        .unwrap()
        .bcs
        .as_ref()
        .unwrap()
        .deserialize()
        .unwrap();
    let (digest, effects) = env.exec_tx_directly(resolved).await.unwrap();
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    redeem_storage_claim(&mut env, funder, &effects).await;
    assert!(env.get_sui_balance_ab(spender) < spender_before - 100_000_000);
}

#[sim_test]
async fn storage_claim_preserves_a_full_user_event_quota() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let spender = env.get_sender(1);
    let funder = env.get_sender(0);
    let allowance = issue(&mut env, spender, FUND).await;
    let (_, created) = env
        .exec_tx_directly(transaction(
            &env,
            spender,
            allowance,
            0,
            200_000_000,
            create_storage(pkg),
        ))
        .await
        .unwrap();
    assert!(created.status().is_ok(), "{:?}", created.status());
    let object = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("allowance-funded object")
        .0;
    let event_limit = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .epoch_store_for_testing()
            .protocol_config()
            .max_num_event_emit()
    });
    let mut pt = ProgrammableTransactionBuilder::new();
    let count = pt.pure(event_limit).unwrap();
    pt.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("emit_many").unwrap(),
        vec![],
        vec![count],
    );
    let object = pt.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
    pt.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![object],
    );
    let tx = TransactionData::new_programmable(
        spender,
        vec![env.get_gas_for_sender(spender)[0]],
        pt.finish(),
        200_000_000,
        env.rgp,
    );
    let signed = env.cluster.wallet.sign_transaction(&tx).await;
    let (effects, events) = env
        .cluster
        .execute_transaction_return_raw_effects(signed)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    env.cluster
        .wait_for_tx_settlement(&[*effects.transaction_digest()])
        .await;
    let user_events = events
        .data
        .iter()
        .filter(|event| {
            event.type_.module.as_str() == "gas_test" && event.type_.name.as_str() == "UserEvent"
        })
        .count();
    assert_eq!(user_events, event_limit as usize);
    redeem_storage_claim(&mut env, funder, &effects).await;
}

#[sim_test]
async fn epoch_change_cannot_redirect_allowance_refunds_to_the_spender() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let spender = env.get_sender(1);
    let allowance = issue(&mut env, spender, FUND).await;
    let validator = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_sui_system_state_object_for_testing()
            .unwrap()
            .into_sui_system_state_summary()
            .active_validators[0]
            .sui_address
    });
    let mut pt = ProgrammableTransactionBuilder::new();
    let system = pt.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
    let coin = pt
        .obj(ObjectArg::ImmOrOwnedObject(
            env.get_gas_for_sender(spender)[0],
        ))
        .unwrap();
    let amount = pt.pure(1_000_000_000u64).unwrap();
    let stake = pt.command(Command::SplitCoins(coin, vec![amount]));
    pt.transfer_arg(spender, coin);
    let validator = pt.pure(validator).unwrap();
    pt.programmable_move_call(
        sui_types::SUI_SYSTEM_PACKAGE_ID,
        Identifier::new("sui_system").unwrap(),
        Identifier::new("request_add_stake").unwrap(),
        vec![],
        vec![system, stake, validator],
    );
    let tx = transaction(&env, spender, allowance, 0, 200_000_000, pt.finish());
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());

    // Clear only the wrapper's attribution before epoch execution touches its inner field.
    let funder = env.get_sender(0);
    let mut deferred_claim = None;
    for after_epoch in [false, true] {
        if after_epoch {
            env.cluster.trigger_reconfiguration().await;
            env.cluster.wait_for_epoch_all_nodes(1).await;
            let system = env
                .cluster
                .get_object_from_fullnode_store(&sui_types::SUI_SYSTEM_STATE_OBJECT_ID)
                .await
                .unwrap();
            let epoch_effects = env.cluster.fullnode_handle.sui_node.with(|node| {
                node.state()
                    .get_transaction_cache_reader()
                    .get_executed_effects(&system.previous_transaction)
                    .unwrap()
            });
            deferred_claim = epoch_effects
                .created()
                .into_iter()
                .find(|(_, owner)| *owner == Owner::AddressOwner(funder))
                .map(|(reference, _)| reference);
        }
        let mut pt = ProgrammableTransactionBuilder::new();
        pt.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
        let tx = TransactionData::new_programmable(
            spender,
            vec![env.get_gas_for_sender(spender)[0]],
            pt.finish(),
            BUDGET,
            env.rgp,
        );
        let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        assert!(
            effects.gas_cost_summary().net_gas_usage() >= 0,
            "The spender recovered an allowance-funded system-state deposit: {:?}",
            effects.gas_cost_summary(),
        );
    }

    let claim = deferred_claim.expect("epoch settlement must retain the funder's refund");
    let mut pt = ProgrammableTransactionBuilder::new();
    let input = pt.obj(ObjectArg::ImmOrOwnedObject(claim)).unwrap();
    pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("claim_storage_rebate").unwrap(),
        vec![],
        vec![input],
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![env.get_gas_for_sender(funder)[0]],
        pt.finish(),
        BUDGET,
        env.rgp,
    );
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(effects.gas_cost_summary().net_gas_usage() < -1_000_000);
    assert!(
        env.cluster
            .get_object_from_fullnode_store(&claim.0)
            .await
            .is_none()
    );
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn disabling_allowance_payments_preserves_existing_storage_provenance() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|version, mut config| {
            if version == ProtocolVersion::MAX_ALLOWED {
                config.set_enable_gas_allowances_for_testing(false);
            }
            config
        }))
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                .with_protocol_version(ProtocolVersion::MAX)
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    ProtocolVersion::MAX.as_u64(),
                    ProtocolVersion::MAX_ALLOWED.as_u64(),
                ))
        }))
        .build()
        .await;
    let initial_config = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .load_epoch_store_one_call_per_task()
            .protocol_config()
            .clone()
    });
    assert!(initial_config.enable_gas_allowances());
    assert!(initial_config.enable_storage_rebate_provenance());

    let pkg = package(&mut env).await;
    let funder = env.get_sender(0);
    let spender = env.get_sender(1);
    let allowance = issue(&mut env, spender, FUND).await;
    let (_, created) = env
        .exec_tx_directly(transaction(
            &env,
            spender,
            allowance,
            0,
            200_000_000,
            create_storage(pkg),
        ))
        .await
        .unwrap();
    assert!(created.status().is_ok(), "{:?}", created.status());
    let object = created
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("allowance-funded object")
        .0;
    assert!(
        env.cluster
            .get_object_from_fullnode_store(&StorageRebate::child_id(object.0))
            .await
            .is_some(),
        "allowance payment records the protected deposit"
    );

    env.trigger_reconfiguration().await;
    env.trigger_reconfiguration().await;
    let transitioned_config = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .load_epoch_store_one_call_per_task()
            .protocol_config()
            .clone()
    });
    assert_eq!(transitioned_config.version, ProtocolVersion::MAX_ALLOWED);
    assert!(!transitioned_config.enable_gas_allowances());
    assert!(transitioned_config.enable_storage_rebate_provenance());

    let current_epoch = env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().load_epoch_store_one_call_per_task().epoch());
    let mut disabled_payment = transaction(
        &env,
        spender,
        allowance,
        1,
        BUDGET,
        ProgrammableTransactionBuilder::new().finish(),
    );
    let TransactionData::V1(data) = &mut disabled_payment;
    data.expiration = TransactionExpiration::ValidDuring {
        min_epoch: Some(current_epoch),
        max_epoch: Some(current_epoch),
        min_timestamp: None,
        max_timestamp: None,
        chain: env.chain_id,
        nonce: 1,
    };
    let error = env
        .exec_tx_directly(disabled_payment)
        .await
        .expect_err("new allowance payments are disabled after the transition");
    assert!(matches!(
        UserInputError::try_from(error),
        Ok(UserInputError::Unsupported(_))
    ));

    env.update_all_gas().await;
    let mut delete = ProgrammableTransactionBuilder::new();
    let input = delete
        .obj(ObjectArg::ImmOrOwnedObject(object))
        .expect("allowance-funded object remains usable with ordinary gas");
    delete.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![input],
    );
    let (_, deleted) = env
        .exec_tx_directly(TransactionData::new_programmable(
            spender,
            vec![env.get_gas_for_sender(spender)[0]],
            delete.finish(),
            BUDGET,
            env.rgp,
        ))
        .await
        .unwrap();
    assert!(deleted.status().is_ok(), "{:?}", deleted.status());
    assert!(
        deleted.gas_cost_summary().net_gas_usage() > 0,
        "ordinary payer must not receive the allowance-funded deposit"
    );
    redeem_storage_claim(&mut env, funder, &deleted).await;
}

#[sim_test]
async fn an_allowance_recycles_its_own_attributed_deposit_without_draining_authorization() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let creator = env.get_sender(1);
    let original_funder = env.get_sender(0);
    let original = issue(&mut env, creator, FUND).await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let mut pt = ProgrammableTransactionBuilder::new();
    let coin = pt
        .obj(ObjectArg::ImmOrOwnedObject(
            env.get_gas_for_sender(creator)[0],
        ))
        .unwrap();
    let amount = pt.pure(FUND).unwrap();
    let payment = pt.command(Command::SplitCoins(coin, vec![amount]));
    pt.transfer_arg(creator, coin);
    let recipient = pt.pure(spender).unwrap();
    pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![],
        vec![payment, recipient],
    );
    let tx = transaction(&env, creator, original, 0, BUDGET, pt.finish());
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let allowance = effects
        .created()
        .into_iter()
        .find_map(|(reference, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((reference.0, initial_shared_version)),
            _ => None,
        })
        .unwrap();
    for nonce in 0..2 {
        let before = state(&env, allowance.0).await.balance.value();
        let creator_before = env.get_sui_balance_ab(creator);
        let original_before = env.get_sui_balance_ab(original_funder);
        let tx = transaction(
            &env,
            spender,
            allowance,
            nonce,
            BUDGET,
            ProgrammableTransactionBuilder::new().finish(),
        );
        let effects = execute(&env, tx, &key).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        if nonce == 0 {
            assert_eq!(env.get_sui_balance_ab(original_funder), original_before);
            redeem_storage_claim(&mut env, original_funder, &effects).await;
        } else {
            let after = state(&env, allowance.0).await.balance.value();
            assert_eq!(
                i128::from(before - after),
                i128::from(effects.gas_cost_summary().net_gas_usage()),
            );
            assert_eq!(env.get_sui_balance_ab(creator), creator_before);
        }
    }
}

#[sim_test]
async fn partial_protected_deposit_preserves_the_ordinary_payers_refund() {
    use sui_types::gas_coin::GasCoin;

    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut env = TestEnvBuilder::new().build().await;
    let pkg = package(&mut env).await;
    let funder = env.get_sender(0);
    let spender = env.get_sender(1);
    let allowance = issue(&mut env, spender, FUND).await;
    let mut create = ProgrammableTransactionBuilder::new();
    let value = create.pure(1u64).unwrap();
    let data = create.pure(vec![7u8; 4_000]).unwrap();
    create.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data],
    );
    let (_, effects) = env
        .exec_tx_directly(TransactionData::new_programmable(
            spender,
            vec![env.get_gas_for_sender(spender)[0]],
            create.finish(),
            200_000_000,
            env.rgp,
        ))
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let object = effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .unwrap()
        .0;
    let unprotected = env
        .cluster
        .get_object_from_fullnode_store(&object.0)
        .await
        .unwrap()
        .storage_rebate;

    let mut grow = ProgrammableTransactionBuilder::new();
    let input = grow.obj(ObjectArg::ImmOrOwnedObject(object)).unwrap();
    let data = grow.pure(vec![7u8; 8_000]).unwrap();
    grow.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("resize_object").unwrap(),
        vec![],
        vec![input, data],
    );
    let (_, effects) = env
        .exec_tx_directly(transaction(
            &env,
            spender,
            allowance,
            0,
            200_000_000,
            grow.finish(),
        ))
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let grown = env
        .cluster
        .get_object_from_fullnode_store(&object.0)
        .await
        .unwrap();
    let marker = env
        .cluster
        .get_object_from_fullnode_store(&StorageRebate::child_id(object.0))
        .await
        .unwrap();
    let protected: StorageRebate =
        bcs::from_bytes(marker.data.try_as_move().unwrap().contents()).unwrap();
    assert!(protected.amount > 0 && protected.amount < grown.storage_rebate);
    assert_eq!(grown.storage_rebate - protected.amount, unprotected);

    let gas = env.get_gas_for_sender(spender)[0];
    let gas_before = env
        .cluster
        .get_object_from_fullnode_store(&gas.0)
        .await
        .unwrap();
    let mut delete = ProgrammableTransactionBuilder::new();
    let input = delete
        .obj(ObjectArg::ImmOrOwnedObject(
            grown.compute_object_reference(),
        ))
        .unwrap();
    delete.programmable_move_call(
        pkg,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![input],
    );
    let (_, effects) = env
        .exec_tx_directly(TransactionData::new_programmable(
            spender,
            vec![gas],
            delete.finish(),
            BUDGET,
            env.rgp,
        ))
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let gas_after = env
        .cluster
        .get_object_from_fullnode_store(&gas.0)
        .await
        .unwrap();
    assert!(effects.gas_cost_summary().net_gas_usage() < 0);
    assert_eq!(
        i128::from(GasCoin::try_from(&gas_before).unwrap().value())
            - i128::from(GasCoin::try_from(&gas_after).unwrap().value()),
        i128::from(effects.gas_cost_summary().net_gas_usage()),
    );
    let claim = env
        .cluster
        .get_object_from_fullnode_store(&StorageRebate::claim_id(
            funder,
            effects.transaction_digest(),
        ))
        .await
        .unwrap();
    assert_eq!(claim.owner, Owner::AddressOwner(funder));
    assert_eq!(
        claim.storage_rebate,
        protected.amount + marker.storage_rebate
    );
    redeem_storage_claim(&mut env, funder, &effects).await;
}
