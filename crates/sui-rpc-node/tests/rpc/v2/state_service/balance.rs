// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the bulk of
//! `sui-e2e-tests/tests/rpc/v2/state_service/balance.rs`.
//!
//! Skipped:
//!
//! - `test_address_balance` and
//!   `test_address_balance_account_with_only_address_balance` —
//!   both flip on accumulators via
//!   `ProtocolConfig::apply_overrides_for_testing`, which is
//!   process-global and would conflict with every other test
//!   sharing this binary's `LocalCluster`.
//! - `test_balance_apis` and the implicit `INITIAL_SUI_BALANCE`
//!   assertions — `TestClusterBuilder` funds its addresses with a
//!   fixed 150-Peta-MIST grant; Simulacrum's `funded_account`
//!   takes the amount as a parameter, so we assert what we asked
//!   for instead of the e2e magic number.

use std::path::PathBuf;

use sui_rpc::proto::sui::rpc::v2::Balance;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_types::Identifier;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas::GasCostSummary;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Argument;
use sui_types::transaction::CallArg;
use sui_types::transaction::Command;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

async fn state_client(cluster: &LocalCluster) -> StateServiceClient<Channel> {
    StateServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// `get_balance` against an address with no coins returns
/// `balance = 0`, and `list_balances` returns an empty list.
#[tokio::test]
async fn fresh_address_returns_zero_balance() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client = state_client(&cluster).await;

    let fresh = SuiAddress::random_for_testing_only();

    let response = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(fresh.to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.balance.unwrap().balance.unwrap(), 0);

    let list_response = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(fresh.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert!(list_response.balances.is_empty());
    assert!(list_response.next_page_token.is_none());
}

/// After Simulacrum funds an account, the SUI balance reported
/// over the RPC matches the funded amount.
#[tokio::test]
async fn funded_account_balance_reflects_initial_grant() {
    let cluster = LocalCluster::new().await.unwrap();
    let funded = 10_000_000_000u64;
    let (address, _kp, _gas) = cluster.funded_account(funded).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    let balance = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner()
        .balance
        .unwrap();
    assert_eq!(
        balance.balance.unwrap(),
        funded,
        "SUI balance for funded account should be the requested amount",
    );
    assert_eq!(balance.coin_balance.unwrap(), funded);
    assert_eq!(
        balance.coin_type.as_deref(),
        Some(SUI_COIN_TYPE),
        "coin_type should round-trip",
    );

    let list = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(address.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        list.balances.len(),
        1,
        "funded account should hold exactly one coin type",
    );
    assert_eq!(list.balances[0], balance);
}

/// Submitting a transfer between two accounts shifts the balance
/// from sender to receiver, net of gas. Mirrors
/// `test_balance_changes_on_transfer` without the
/// `INITIAL_SUI_BALANCE` magic number — we read the pre-transfer
/// balances explicitly and assert deltas.
#[tokio::test]
async fn balance_changes_on_transfer() {
    let cluster = LocalCluster::new().await.unwrap();

    let initial = 50_000_000_000u64;
    let (sender, sender_kp, sender_gas) = cluster.funded_account(initial).await.unwrap();
    let (receiver, _, _) = cluster.funded_account(initial).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let transfer_amount = 1_000_000u64;
    let rgp = cluster.reference_gas_price().await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(receiver, Some(transfer_amount));
    let pt = builder.finish();
    let tx_data = TransactionData::new_programmable(sender, vec![sender_gas], pt, 5_000_000, rgp);
    let signed = to_sender_signed_transaction(tx_data, &sender_kp);
    let (effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "transfer must succeed: {err:?}");
    let gas_used = compute_gas_used(effects.gas_cost_summary());
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    verify_balances(
        &mut client,
        sender,
        &[balance_proto(
            SUI_COIN_TYPE,
            initial - transfer_amount - gas_used,
        )],
    )
    .await;
    verify_balances(
        &mut client,
        receiver,
        &[balance_proto(SUI_COIN_TYPE, initial + transfer_amount)],
    )
    .await;
}

/// Three transfers that all post in the same checkpoint settle
/// to the expected per-address net deltas. Mirrors
/// `test_multiple_concurrent_balance_changes`. We sign / submit
/// from a single thread (Simulacrum is single-threaded), then
/// create one checkpoint covering all three transactions — the
/// "concurrent" part of the original test is really about
/// indexing many balance deltas at once.
#[tokio::test]
async fn multiple_concurrent_balance_changes() {
    let cluster = LocalCluster::new().await.unwrap();

    // Each account needs two coins so it can spend one as gas and
    // split from the second. Fund each twice.
    let initial = 50_000_000_000u64;
    let (addr_0, kp_0, gas_0a) = cluster.funded_account(initial).await.unwrap();
    let coin_0 = grant_extra_coin(&cluster, addr_0, initial).await;
    let (addr_1, kp_1, gas_1a) = cluster.funded_account(initial).await.unwrap();
    let coin_1 = grant_extra_coin(&cluster, addr_1, initial).await;
    let (addr_2, kp_2, gas_2a) = cluster.funded_account(initial).await.unwrap();
    let coin_2 = grant_extra_coin(&cluster, addr_2, initial).await;
    cluster.create_checkpoint().await.unwrap();

    let xfer_0_to_1 = 5_000_000u64;
    let xfer_1_to_2 = 3_000_000u64;
    let xfer_2_to_1 = 1_000_000u64;
    let rgp = cluster.reference_gas_price().await;

    let gas_0 = split_and_transfer(
        &cluster,
        addr_0,
        &kp_0,
        gas_0a,
        coin_0,
        addr_1,
        xfer_0_to_1,
        rgp,
    )
    .await;
    let gas_1 = split_and_transfer(
        &cluster,
        addr_1,
        &kp_1,
        gas_1a,
        coin_1,
        addr_2,
        xfer_1_to_2,
        rgp,
    )
    .await;
    let gas_2 = split_and_transfer(
        &cluster,
        addr_2,
        &kp_2,
        gas_2a,
        coin_2,
        addr_1,
        xfer_2_to_1,
        rgp,
    )
    .await;
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    // addr_0 paid gas + sent xfer_0_to_1; had `initial * 2` before.
    verify_balances(
        &mut client,
        addr_0,
        &[balance_proto(
            SUI_COIN_TYPE,
            initial * 2 - xfer_0_to_1 - gas_0,
        )],
    )
    .await;

    // addr_1 received xfer_0_to_1 + xfer_2_to_1, sent xfer_1_to_2,
    // paid gas; had `initial * 2` before.
    verify_balances(
        &mut client,
        addr_1,
        &[balance_proto(
            SUI_COIN_TYPE,
            initial * 2 + xfer_0_to_1 - xfer_1_to_2 + xfer_2_to_1 - gas_1,
        )],
    )
    .await;

    // addr_2 received xfer_1_to_2, sent xfer_2_to_1, paid gas;
    // had `initial * 2` before.
    verify_balances(
        &mut client,
        addr_2,
        &[balance_proto(
            SUI_COIN_TYPE,
            initial * 2 + xfer_1_to_2 - xfer_2_to_1 - gas_2,
        )],
    )
    .await;
}

/// Publish `trusted_coin`, mint some, transfer a slice to a
/// second address, and assert balances for both addresses across
/// SUI and the custom coin type. Mirrors `test_custom_coin_balance`.
#[tokio::test]
async fn custom_coin_balance() {
    let cluster = LocalCluster::new().await.unwrap();

    let initial = 1_000_000_000_000u64;
    let (sender, kp, gas) = cluster.funded_account(initial).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, publish_effects) = cluster
        .publish_package(sender, &kp, gas, trusted_coin_path())
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let publish_gas = compute_gas_used(publish_effects.gas_cost_summary());
    let post_publish_gas = publish_effects.gas_object().unwrap().0;

    // Find the TreasuryCap address-owned object created by init.
    let treasury_cap = find_object_by_type(&cluster, &publish_effects, |obj_type| {
        let s = obj_type.to_canonical_string(true);
        s.contains("::coin::TreasuryCap<") && s.contains("::trusted_coin::TRUSTED_COIN>")
    })
    .await;

    let rgp = cluster.reference_gas_price().await;
    let mint_amount = 1_000_000u64;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id,
            Identifier::new("trusted_coin").unwrap(),
            Identifier::new("mint").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap)),
                CallArg::Pure(bcs::to_bytes(&mint_amount).unwrap()),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let tx_data =
        TransactionData::new_programmable(sender, vec![post_publish_gas], pt, 50_000_000, rgp);
    let (mint_effects, err) = cluster
        .execute_transaction(to_sender_signed_transaction(tx_data, &kp))
        .await
        .unwrap();
    assert!(err.is_none(), "mint must succeed: {err:?}");
    let mint_gas = compute_gas_used(mint_effects.gas_cost_summary());
    let post_mint_gas = mint_effects.gas_object().unwrap().0;
    cluster.create_checkpoint().await.unwrap();

    let coin_type = format!("{}::trusted_coin::TRUSTED_COIN", package_id);

    let mut client = state_client(&cluster).await;

    verify_balances(
        &mut client,
        sender,
        &[
            balance_proto(SUI_COIN_TYPE, initial - publish_gas - mint_gas),
            balance_proto(&coin_type, mint_amount),
        ],
    )
    .await;

    // Find the freshly minted Coin<TRUSTED_COIN>.
    let trusted_coin_ref = find_object_by_type(&cluster, &mint_effects, |obj_type| {
        let s = obj_type.to_canonical_string(true);
        s.contains("::coin::Coin<") && s.contains("::trusted_coin::TRUSTED_COIN>")
    })
    .await;

    // Split + transfer half to a second address.
    let (recipient, _, _) = cluster.funded_account(initial).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let transfer_amount = 300_000u64;

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(trusted_coin_ref))
        .unwrap();
    let amt = builder.pure(transfer_amount).unwrap();
    let split = builder.command(Command::SplitCoins(coin_arg, vec![amt]));
    let piece = match split {
        Argument::Result(i) => Argument::NestedResult(i, 0),
        _ => panic!("split should be a Result"),
    };
    builder.transfer_arg(recipient, piece);
    let pt = builder.finish();
    let tx_data =
        TransactionData::new_programmable(sender, vec![post_mint_gas], pt, 50_000_000, rgp);
    let (xfer_effects, err) = cluster
        .execute_transaction(to_sender_signed_transaction(tx_data, &kp))
        .await
        .unwrap();
    assert!(err.is_none(), "split + transfer must succeed: {err:?}");
    let xfer_gas = compute_gas_used(xfer_effects.gas_cost_summary());
    cluster.create_checkpoint().await.unwrap();

    verify_balances(
        &mut client,
        sender,
        &[
            balance_proto(SUI_COIN_TYPE, initial - publish_gas - mint_gas - xfer_gas),
            balance_proto(&coin_type, mint_amount - transfer_amount),
        ],
    )
    .await;
    verify_balances(
        &mut client,
        recipient,
        &[
            balance_proto(SUI_COIN_TYPE, initial),
            balance_proto(&coin_type, transfer_amount),
        ],
    )
    .await;

    // A third address has no balance for the custom coin (no
    // error since the coin type exists).
    let third = SuiAddress::random_for_testing_only();
    let response = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(third.to_string());
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.balance.as_ref().unwrap().balance.unwrap(),
        0,
        "fresh address should report 0 balance for an existing coin type",
    );
}

/// The InvalidArgument-coded error paths from the e2e test:
/// missing owner, missing coin_type, malformed owner, malformed
/// coin_type, and a corrupted page_token.
#[tokio::test]
async fn invalid_requests_surface_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    let (address, _kp, _gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    // Missing owner.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Missing coin_type.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing coin_type"),
        "unexpected error message: {}",
        err.message(),
    );

    // Invalid address.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some("not_a_hex_address".to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Invalid coin type.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req.coin_type = Some("invalid::coin::type::format".to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid coin_type"),
        "unexpected error message: {}",
        err.message(),
    );

    // `list_balances` missing owner.
    let err = client
        .list_balances(ListBalancesRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Corrupt page token.
    let err = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(address.to_string());
            req.page_token = Some(vec![0xFF, 0xDE, 0xAD, 0xBE, 0xEF].into());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

fn compute_gas_used(summary: &GasCostSummary) -> u64 {
    summary.computation_cost + summary.storage_cost - summary.storage_rebate
}

fn balance_proto(coin_type: &str, amount: u64) -> Balance {
    let mut b = Balance::default();
    b.coin_type = Some(coin_type.to_owned());
    b.balance = Some(amount);
    b.coin_balance = Some(amount);
    b
}

async fn verify_balances(
    client: &mut StateServiceClient<Channel>,
    address: SuiAddress,
    expected_balances: &[Balance],
) {
    for expected in expected_balances {
        let actual = client
            .get_balance({
                let mut req = GetBalanceRequest::default();
                req.owner = Some(address.to_string());
                req.coin_type = expected.coin_type.clone();
                req
            })
            .await
            .unwrap()
            .into_inner()
            .balance
            .unwrap();
        assert_eq!(
            actual,
            *expected,
            "balance mismatch for {} at {}: expected {:?}, got {:?}",
            expected.coin_type.as_deref().unwrap_or(""),
            address,
            expected,
            actual,
        );
    }

    let list = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(address.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(list.balances.len(), expected_balances.len());
    for expected in expected_balances {
        let found = list
            .balances
            .iter()
            .find(|b| b.coin_type == expected.coin_type)
            .unwrap_or_else(|| {
                panic!(
                    "coin type {} missing from list_balances",
                    expected.coin_type.as_deref().unwrap_or(""),
                )
            });
        assert_eq!(found, expected);
    }
}

/// Request an additional gas coin grant for an existing address.
/// Used to give an account a second coin so it can split from
/// one while paying gas with the other.
async fn grant_extra_coin(cluster: &LocalCluster, address: SuiAddress, amount: u64) -> ObjectRef {
    let effects = cluster.request_gas(address, amount).await.unwrap();
    effects
        .created()
        .into_iter()
        .find_map(|(oref, owner)| {
            matches!(owner, Owner::AddressOwner(a) if a == address).then_some(oref)
        })
        .expect("request_gas should create a coin owned by the address")
}

/// Split `amount` off `coin` and transfer the slice to `recipient`,
/// paying with `gas`. Returns the gas used.
#[allow(clippy::too_many_arguments)]
async fn split_and_transfer(
    cluster: &LocalCluster,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas: ObjectRef,
    coin: ObjectRef,
    recipient: SuiAddress,
    amount: u64,
    rgp: u64,
) -> u64 {
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    let amt = builder.pure(amount).unwrap();
    let split = builder.command(Command::SplitCoins(coin_arg, vec![amt]));
    let piece = match split {
        Argument::Result(i) => Argument::NestedResult(i, 0),
        _ => panic!("split should be a Result"),
    };
    builder.transfer_arg(recipient, piece);
    let pt = builder.finish();
    let tx_data = TransactionData::new_programmable(sender, vec![gas], pt, 50_000_000, rgp);
    let (effects, err) = cluster
        .execute_transaction(to_sender_signed_transaction(tx_data, keypair))
        .await
        .unwrap();
    assert!(err.is_none(), "split + transfer must succeed: {err:?}");
    compute_gas_used(effects.gas_cost_summary())
}

/// Resolve the path of the `trusted_coin` Move source tree
/// stored under `sui-e2e-tests`. Reusing it here avoids
/// duplicating the Move source.
fn trusted_coin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sui-e2e-tests")
        .join("tests")
        .join("rpc")
        .join("data")
        .join("trusted_coin")
}

/// Find the single created object whose Move type satisfies
/// `pred`. The publish + mint flows here create one such object;
/// `unwrap` matches the e2e helpers' shape.
async fn find_object_by_type(
    cluster: &LocalCluster,
    effects: &TransactionEffects,
    pred: impl Fn(&sui_types::base_types::MoveObjectType) -> bool,
) -> ObjectRef {
    for (oref, _) in effects.created().into_iter().chain(effects.mutated()) {
        let Some(obj) = cluster.get_object(oref.0).await else {
            continue;
        };
        if obj.type_().map(&pred).unwrap_or(false) {
            return oref;
        }
    }
    panic!("no created/mutated object matched the requested predicate");
}
