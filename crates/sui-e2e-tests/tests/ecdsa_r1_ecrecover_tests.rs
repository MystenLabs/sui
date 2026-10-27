// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `secp256r1_ecrecover` under both settings of `secp256r1_ecrecover_checks_x_below_p`.

use fastcrypto::encoding::{Encoding, Hex};
use move_core_types::identifier::Identifier;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{TransactionData, TransactionKind};
use test_cluster::TestClusterBuilder;

// The Sha256 vector from `ecdsa_r1_tests.move` with r shifted up by 2^256 - n and marked
// x-reduced, so r + n overflows.
const WRAPPED_SIG: &str = "26d84721652d8bc3ddd1986434a10b3bbe82f635b3002820a4c4a259be063fde4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb202";

async fn ecrecover_status(checks_x_below_p: bool) -> ExecutionStatus {
    let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut cfg| {
        cfg.set_secp256r1_ecrecover_checks_x_below_p_for_testing(checks_x_below_p);
        cfg
    });
    let cluster = TestClusterBuilder::new().build().await;
    let sender = cluster.get_address_0();
    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let rgp = cluster.get_reference_gas_price().await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let sig = ptb.pure(Hex::decode(WRAPPED_SIG).unwrap()).unwrap();
    let msg = ptb.pure(b"Hello, world!".to_vec()).unwrap();
    let hash = ptb.pure(1u8).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("ecdsa_r1").unwrap(),
        Identifier::new("secp256r1_ecrecover").unwrap(),
        vec![],
        vec![sig, msg, hash],
    );
    let tx = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        50_000_000,
        rgp,
    );
    let (_, effects) = cluster
        .sign_and_execute_transaction_directly(&tx)
        .await
        .unwrap();
    effects.status().clone()
}

#[sim_test]
async fn ecrecover_rejects_overflowing_x_when_enabled() {
    // Abort code 0 is `ecdsa_r1::EFailToRecoverPubKey`.
    assert!(matches!(
        ecrecover_status(true).await,
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(_, 0),
            ..
        })
    ));
}

#[sim_test]
async fn ecrecover_keeps_legacy_result_when_disabled() {
    assert_eq!(ecrecover_status(false).await, ExecutionStatus::Success);
}
