// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the gRPC SimulateTransaction auto-gasless behavior.
//!
//! When the caller submits an unresolved transaction that is gasless-eligible and does not set
//! an explicit `gas_payment.price`, the simulate flow should auto-switch to `price = 0` so that
//! the returned resolved transaction reflects the gasless shape.

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::u256::U256;
use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_types::balance::Balance;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    self, FundsWithdrawalArg, GasData, ObjectArg, SharedObjectMutability, TransactionData,
    TransactionDataV1, TransactionExpiration, TransactionKind,
};
use sui_types::{MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

async fn setup_gasless_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_gasless_for_testing();
            cfg
        }))
        .build()
        .await
}

async fn setup_mintable_coin_env(
    test_env: &mut TestEnv,
    min_transfer: u64,
    mints: &[(u64, SuiAddress)],
) -> (TypeTag, Vec<ObjectRef>) {
    let (publisher, package_id, coin_type, mut treasury_cap_ref) =
        test_env.setup_mintable_coin().await;

    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true), min_transfer);

    let mut coin_refs = Vec::new();
    for &(amount, recipient) in mints {
        let (new_tcap, coin_ref) = test_env
            .mint_coin(publisher, package_id, treasury_cap_ref, amount, recipient)
            .await;
        treasury_cap_ref = new_tcap;
        coin_refs.push(coin_ref);
    }
    (coin_type, coin_refs)
}

/// Wrap a PTB into a gasless-shaped `TransactionData`. Gas fields are placeholders —
/// to_unresolved_proto strips them before send.
fn wrap_tx(sender: SuiAddress, kind: TransactionKind) -> TransactionData {
    TransactionData::V1(TransactionDataV1 {
        kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 0,
            budget: 0,
        },
        expiration: TransactionExpiration::None,
    })
}

/// Build a gasless-eligible PTB that calls `0x2::coin::send_funds(coin, recipient)` (an
/// allowlisted move call on an allowlisted `Coin<T>` input).
fn build_coin_send_funds_tx(
    sender: SuiAddress,
    coin_ref: ObjectRef,
    coin_type: TypeTag,
    recipient: SuiAddress,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type],
        vec![coin_arg, recipient_arg],
    );
    wrap_tx(
        sender,
        TransactionKind::ProgrammableTransaction(builder.finish()),
    )
}

/// Build a gasless-eligible PTB that withdraws `amount` of `coin_type` from the sender's
/// address balance and forwards it to `recipient` via `balance::send_funds`. The PTB has *no*
/// `Coin<T>` object inputs — its only `CallArg`s are a `FundsWithdrawal` and a `Pure` recipient
/// — so replay protection cannot come from address-owned object inputs and must be supplied by
/// the transaction's `expiration`.
fn build_balance_withdrawal_send_funds_tx(
    sender: SuiAddress,
    coin_type: TypeTag,
    amount: u64,
    recipient: SuiAddress,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, coin_type.clone());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![coin_type.clone()],
        vec![withdraw_arg],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type],
        vec![balance, recipient_arg],
    );
    wrap_tx(
        sender,
        TransactionKind::ProgrammableTransaction(builder.finish()),
    )
}

/// Turn a `TransactionData` into an unresolved-path proto request: drop BCS (so the resolver
/// runs), clear gas payment objects/budget, and set gas price to `price` (None = "caller didn't
/// specify"). Expiration is cleared because the resolve path only decodes None/Epoch; the
/// `Coin<T>` input's address ownership provides replay protection.
fn to_unresolved_proto(tx: TransactionData, price: Option<u64>) -> Transaction {
    let mut proto = Transaction::from(tx);
    proto.bcs = None;
    proto.digest = None;
    proto.expiration = None;
    if let Some(gp) = proto.gas_payment.as_mut() {
        gp.objects.clear();
        gp.budget = None;
        gp.price = price;
    }
    proto
}

async fn connect(
    test_env: &TestEnv,
) -> TransactionExecutionServiceClient<tonic::transport::Channel> {
    TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
        .await
        .unwrap()
}

#[sim_test]
async fn simulate_auto_switches_to_gasless_when_eligible() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let (coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, 0, &[(5_000, sender)]).await;
    let tx = build_coin_send_funds_tx(sender, coin_refs[0], coin_type, recipient);

    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, None))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(gas_payment.price, Some(0));
    assert_eq!(gas_payment.budget, Some(0));
    assert!(gas_payment.objects.is_empty());
    assert!(returned.effects().status().success());
}

#[sim_test]
async fn simulate_respects_explicit_price_over_gasless() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let (coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, 0, &[(5_000, sender)]).await;
    let tx = build_coin_send_funds_tx(sender, coin_refs[0], coin_type, recipient);

    let rgp = test_env.rgp;
    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, Some(rgp)))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(gas_payment.price, Some(rgp));
    assert!(
        !gas_payment.objects.is_empty(),
        "priced flow must have run gas selection",
    );
    assert!(returned.effects().status().success());
}

/// Explicitly setting gas_price=0 on a gasless-eligible tx must be accepted: the API should
/// treat `price=0` the same as an unset price for purposes of gasless candidacy.
#[sim_test]
async fn simulate_explicit_gas_price_zero_accepted_for_gasless_eligible_tx() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let (coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, 0, &[(5_000, sender)]).await;
    let tx = build_coin_send_funds_tx(sender, coin_refs[0], coin_type, recipient);

    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, Some(0)))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(gas_payment.price, Some(0));
    assert_eq!(gas_payment.budget, Some(0));
    assert!(gas_payment.objects.is_empty());
    assert!(returned.effects().status().success());
}

/// A fully-formed BCS gasless transaction (price=0, budget=0, no payment objects) must be
/// accepted when do_gas_selection=true. The BCS path bypasses is_gasless_candidate to avoid
/// second-guessing an explicit caller choice, but a BCS transaction already in gasless shape
/// should be simulated as-is rather than being routed through the priced flow (which would
/// fail because gas_price=0 < RGP).
#[sim_test]
async fn simulate_bcs_gasless_transaction_accepted() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let (coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, 0, &[(5_000, sender)]).await;
    // wrap_tx already produces price=0, budget=0, payment=[] — a valid gasless shape.
    // The Coin<T> input provides address-owned replay protection, so no ValidDuring needed.
    let tx = build_coin_send_funds_tx(sender, coin_refs[0], coin_type, recipient);

    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(Transaction::from(tx)).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(gas_payment.price, Some(0));
    assert_eq!(gas_payment.budget, Some(0));
    assert!(gas_payment.objects.is_empty());
    assert!(returned.effects().status().success());
}

/// A tx whose `Coin<T>` input is a type *not* on the gasless allowlist must not be classified
/// as gasless-eligible: `is_gasless_candidate` should reject it via `check_gasless_inputs`,
/// and the simulate flow should fall through to the priced flow rather than returning
/// `gas_price=0`.
#[sim_test]
async fn simulate_falls_back_to_priced_when_coin_type_not_allowlisted() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    // Mint a coin for sender but deliberately skip `add_gasless_token_for_testing`, so the
    // coin type is absent from `gasless_allowed_token_types`.
    let (publisher, package_id, coin_type, treasury_cap_ref) = test_env.setup_mintable_coin().await;
    let (_, coin_ref) = test_env
        .mint_coin(publisher, package_id, treasury_cap_ref, 5_000, sender)
        .await;
    let tx = build_coin_send_funds_tx(sender, coin_ref, coin_type, recipient);

    let rgp = test_env.rgp;
    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, None))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(
        gas_payment.price,
        Some(rgp),
        "non-allowlisted coin type must not be classified as gasless-eligible; \
         priced fallback should resolve price to rgp",
    );
    assert!(
        !gas_payment.objects.is_empty(),
        "fallback priced flow must have run gas selection",
    );
    assert!(returned.effects().status().success());
}

/// A tx that passes structural + runtime-input gasless checks but fails the post-execution
/// gasless requirements should transparently fall back to the priced flow. Here the tx tries
/// to send a coin balance below the registered gasless minimum for its token type — the
/// structural and input checks all pass (`Coin<T>` is in the allowlist), but
/// check_gasless_execution_requirements fails post-execution. The priced flow has no such
/// minimum and succeeds.
#[sim_test]
async fn simulate_falls_back_to_priced_when_gasless_post_exec_fails() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    // Register a minimum transfer of 10_000 but mint a coin with only 5_000 — gasless post-exec
    // check rejects transfers below the minimum.
    let (coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, 10_000, &[(5_000, sender)]).await;
    let tx = build_coin_send_funds_tx(sender, coin_refs[0], coin_type, recipient);

    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, None))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_ne!(
        gas_payment.price,
        Some(0),
        "post-exec gasless failure must fall back to priced flow, not return gasless shape",
    );
    assert!(
        !gas_payment.objects.is_empty(),
        "fallback priced flow must have run gas selection",
    );
    assert!(returned.effects().status().success());
}

/// A gasless-eligible PTB whose only `CallArg`s are a `FundsWithdrawal` (withdrawing from the
/// sender's address balance) and a `Pure` recipient — i.e. it has zero address-owned object
/// inputs — must still be simulatable. Replay protection in this case can only come from the
/// transaction's `expiration`, which the caller never sets when going through the unresolved
/// proto path. The simulate flow therefore needs to fill in a `ValidDuring` expiration before
/// invoking the executor; otherwise `check_replay_protection` rejects the tx with
/// "Transactions must either have address-owned inputs, or a ValidDuring expiration".
#[sim_test]
async fn simulate_gasless_with_no_address_owned_inputs_succeeds() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let sui_type = GAS::type_tag();
    transaction::add_gasless_token_for_testing(sui_type.to_canonical_string(true), 0);
    // Seed sender's SUI address balance. `fund_one_address_balance` updates the gas ref from
    // the tx effects (rather than the lagging RPC indexer), keeping the env's view in sync.
    test_env.fund_one_address_balance(sender, 5_000).await;

    let tx = build_balance_withdrawal_send_funds_tx(sender, sui_type, 1_000, recipient);

    let response = connect(&test_env)
        .await
        .simulate_transaction(
            SimulateTransactionRequest::new(to_unresolved_proto(tx, None))
                .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let returned = response.transaction();
    let gas_payment = returned.transaction().gas_payment();
    assert_eq!(gas_payment.price, Some(0));
    assert_eq!(gas_payment.budget, Some(0));
    assert!(gas_payment.objects.is_empty());
    assert!(returned.effects().status().success());
}

/// Issues a SUI allowance from `funder` to `spender` that expires at `expiration_ms`, returning
/// its id and initial shared version.
async fn issue_sui_allowance(
    test_env: &mut TestEnv,
    funder: SuiAddress,
    spender: SuiAddress,
    expiration_ms: u64,
) -> (ObjectID, SequenceNumber) {
    let mut builder = ProgrammableTransactionBuilder::new();
    let rate_limit = builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        Identifier::new("option").unwrap(),
        Identifier::new("none").unwrap(),
        vec!["0x2::allowance::RateLimit".parse().unwrap()],
        vec![],
    );
    let args = vec![
        builder.pure("".to_string()).unwrap(),
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(1_000u64))).unwrap(),
        builder.pure(None::<u64>).unwrap(),
        builder.pure(Some(expiration_ms)).unwrap(),
        rate_limit,
    ];
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![Balance::type_tag(GAS::type_tag())],
        args,
    );
    let funder_gas = test_env.get_gas_for_sender(funder)[0];
    let tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    test_env.update_all_gas().await;
    effects
        .created()
        .iter()
        .find_map(|(obj_ref, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((obj_ref.0, *initial_shared_version)),
            _ => None,
        })
        .expect("the allowance is created as a shared object")
}

/// Builds a gasless-shaped spend of 100 MIST from the allowance, sent back to the funder.
fn build_allowance_spend_tx(
    spender: SuiAddress,
    funder: SuiAddress,
    (allowance_id, initial_shared_version): (ObjectID, SequenceNumber),
) -> TransactionData {
    let sui_type = GAS::type_tag();
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
            100,
            sui_type.clone(),
            funder,
            allowance_id,
        ))
        .unwrap();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let clock_arg = builder.input(transaction::CallArg::CLOCK_IMM).unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("balance_spend").unwrap(),
        vec![sui_type.clone()],
        vec![allowance_arg, withdraw_arg, clock_arg],
    );
    let recipient_arg = builder.pure(funder).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![sui_type],
        vec![balance, recipient_arg],
    );
    wrap_tx(
        spender,
        TransactionKind::ProgrammableTransaction(builder.finish()),
    )
}

/// A spend from an expired allowance fails the gasless input checks, which refuse spends that
/// `consume` must abort. Simulate must not classify it as gasless, so it falls back to the priced
/// flow and reports the Move abort instead of an error. A spend from a live allowance stays
/// gasless.
#[sim_test]
async fn simulate_falls_back_to_priced_for_expired_allowance_spend() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_gasless_for_testing();
            cfg.set_gasless_allowance_spend_for_testing(true);
            cfg
        }))
        .build()
        .await;
    let funder = test_env.get_sender(0);
    let spender = test_env.get_sender(1);
    transaction::add_gasless_token_for_testing(GAS::type_tag().to_canonical_string(true), 0);
    test_env.fund_one_address_balance(funder, 5_000).await;
    let live = issue_sui_allowance(&mut test_env, funder, spender, u64::MAX).await;
    let expired = issue_sui_allowance(&mut test_env, funder, spender, 1).await;

    let mut client = connect(&test_env).await;
    let mut simulate = async |allowance, price| {
        let tx = build_allowance_spend_tx(spender, funder, allowance);
        client
            .simulate_transaction(
                SimulateTransactionRequest::new(to_unresolved_proto(tx, price))
                    .with_do_gas_selection(true),
            )
            .await
            .unwrap()
            .into_inner()
    };

    let response = simulate(live, None).await;
    let returned = response.transaction();
    assert_eq!(returned.transaction().gas_payment().price, Some(0));
    assert!(returned.effects().status().success());

    // A caller echoing a previous gasless price of 0 gets the same priced fallback.
    for price in [None, Some(0)] {
        let response = simulate(expired, price).await;
        let returned = response.transaction();
        let gas_payment = returned.transaction().gas_payment();
        assert_eq!(gas_payment.price, Some(test_env.rgp));
        assert!(!gas_payment.objects.is_empty());
        let status = returned.effects().status();
        assert!(!status.success());
        let abort = status.error().abort();
        assert_eq!(
            abort.location().module(),
            "allowance",
            "{:?}",
            status.error()
        );
        assert!(
            abort.clever_error().constant_name().contains("EExpired"),
            "{:?}",
            status.error()
        );
    }
}
