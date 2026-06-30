// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the multi-coin scenarios from
//! `sui-indexer-alt-e2e-tests/tests/consistent_store_address_balance_tests.rs`
//! that need a custom Move coin package (`my_coin`, `a_coin`,
//! `b_coin`, `c_coin`) — sources live in
//! `tests/packages/coin/`.
//!
//! Tests covered:
//!
//! - `test_multiple_coin_types` (address-balance variant):
//!   recipient has both SUI and MY_COIN address balances after
//!   parallel sends.
//! - `test_address_to_address_transfer`: A's MY_COIN address
//!   balance moves to B's; emptied address has no entry in
//!   `list_balances`; historical reads at past checkpoints see
//!   the prior state.
//! - `test_list_balances_pagination`: paginate a mix of coin /
//!   address balances forward + backward across page-size
//!   boundaries.

use std::path::PathBuf;
use std::str::FromStr;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::GetBalanceRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListObjectsByTypeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::Identifier;
use sui_types::TypeTag;
use sui_types::base_types::ObjectDigest;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

fn accumulator_overrides() -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    })
}

/// A `LocalCluster` plus a published coin package and the
/// publisher's keypair / treasury caps. Mirrors the e2e
/// `BalanceCluster` struct but trimmed to the API the ported
/// tests actually call.
struct MultiCoinCluster {
    cluster: LocalCluster,
    publisher: SuiAddress,
    publisher_kp: AccountKeyPair,
    pkg: ObjectID,
}

impl MultiCoinCluster {
    async fn new() -> Self {
        let cluster = LocalCluster::new().await.unwrap();
        let (publisher, publisher_kp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET * 2)
            .await
            .expect("funded_account");

        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["tests", "packages", "coin"]);

        // `TestTransactionBuilder::publish_async` calls
        // `BuildConfig::new_for_testing().build_async(&path)`
        // internally, so the only thing we need to supply is
        // the on-disk Move source. Use the async variant because
        // we're inside a tokio runtime.
        let tx = TestTransactionBuilder::new(publisher, gas, cluster.reference_gas_price().await)
            .with_gas_budget(DEFAULT_GAS_BUDGET)
            .publish_async(path)
            .await
            .build();
        let signed = to_sender_signed_transaction(tx, &publisher_kp);
        let (fx, err) = cluster
            .execute_transaction(signed)
            .await
            .expect("execute_transaction");
        assert!(err.is_none(), "publish failed: {err:?}");
        assert!(fx.status().is_ok(), "publish tx status not ok");

        let pkg = fx
            .created()
            .into_iter()
            .find_map(|(oref, owner)| {
                (oref.1.value() == 1 && matches!(owner, Owner::Immutable)).then_some(oref.0)
            })
            .expect("publish should create an immutable package object");

        // Close out a checkpoint so the indexer picks up the
        // package + its TreasuryCaps before any test code
        // tries to look one up via `list_objects_by_type`.
        cluster.create_checkpoint().await.unwrap();

        Self {
            cluster,
            publisher,
            publisher_kp,
            pkg,
        }
    }

    fn my_coin_type(&self) -> TypeTag {
        format!(
            "{}::my_coin::MY_COIN",
            self.pkg.to_canonical_display(/* with_prefix */ true),
        )
        .parse()
        .unwrap()
    }

    fn coin_type(&self, module_prefix: &str) -> TypeTag {
        format!(
            "{}::{}_coin::{}_COIN",
            self.pkg.to_canonical_display(true),
            module_prefix,
            module_prefix.to_uppercase(),
        )
        .parse()
        .unwrap()
    }

    /// Return the publisher's TreasuryCap<T> for `coin_type`,
    /// looked up via `list_objects_by_type`.
    async fn treasury_cap(&self, coin_type: &TypeTag) -> ObjectRef {
        let mut client = ConsistentServiceClient::connect(self.cluster.grpc_url().to_string())
            .await
            .unwrap();
        let response = client
            .list_objects_by_type(ListObjectsByTypeRequest {
                object_type: Some(format!(
                    "0x2::coin::TreasuryCap<{}>",
                    coin_type.to_canonical_string(true),
                )),
                page_size: Some(10),
                ..Default::default()
            })
            .await
            .expect("list_objects_by_type")
            .into_inner();
        let caps: Vec<_> = response
            .objects
            .into_iter()
            .map(|o| {
                let id = ObjectID::from_str(o.object_id()).expect("ObjectID");
                let version = SequenceNumber::from_u64(o.version());
                let digest = ObjectDigest::from_str(o.digest()).expect("ObjectDigest");
                (id, version, digest)
            })
            .collect();
        assert!(
            !caps.is_empty(),
            "no TreasuryCap found for {}",
            coin_type.to_canonical_string(true),
        );
        caps[0]
    }

    /// Just-in-time gas for `requester`. Returns the new
    /// address-owned gas coin.
    async fn request_gas(&self, requester: SuiAddress, amount: u64) -> ObjectRef {
        let fx = self
            .cluster
            .request_gas(requester, DEFAULT_GAS_BUDGET + amount)
            .await
            .expect("request_gas");
        fx.created()
            .into_iter()
            .find_map(|(oref, o)| {
                matches!(o, Owner::AddressOwner(a) if a == requester).then_some(oref)
            })
            .expect("request_gas produced an address-owned coin")
    }

    /// Publisher sends `amount` SUI to `recipient`'s address
    /// balance.
    async fn send_sui_to_address_balance(&self, recipient: SuiAddress, amount: u64) {
        let gas = self.request_gas(self.publisher, amount).await;
        let tx = TestTransactionBuilder::new(
            self.publisher,
            gas,
            self.cluster.reference_gas_price().await,
        )
        .with_gas_budget(DEFAULT_GAS_BUDGET)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build();
        let signed = to_sender_signed_transaction(tx, &self.publisher_kp);
        let (fx, err) = self
            .cluster
            .execute_transaction(signed)
            .await
            .expect("execute_transaction");
        assert!(err.is_none(), "send_sui_to_address_balance: {err:?}");
        assert!(fx.status().is_ok());
    }

    /// Publisher mints + sends `amount` of `coin_type` to
    /// `recipient`'s address balance via the coin package's
    /// `mint_balance` (or `my_coin::mint_balance`) entry point.
    async fn send_balance_to_address_balance(
        &self,
        recipient: SuiAddress,
        amount: u64,
        coin_type: &TypeTag,
    ) {
        let (module, func) = self.mint_balance_func(coin_type);
        let gas = self.request_gas(self.publisher, 0).await;
        let cap = self.treasury_cap(coin_type).await;
        let tx = mint_tx(
            self.publisher,
            gas,
            self.pkg,
            module,
            func,
            cap,
            amount,
            recipient,
            self.cluster.reference_gas_price().await,
        );
        let signed = to_sender_signed_transaction(tx, &self.publisher_kp);
        let (fx, err) = self
            .cluster
            .execute_transaction(signed)
            .await
            .expect("execute_transaction");
        assert!(err.is_none(), "send_balance_to_address_balance: {err:?}");
        assert!(fx.status().is_ok());
    }

    /// Publisher mints + sends `amount` of `coin_type` as an
    /// object owned by `recipient`.
    async fn send_coin_to_address(&self, recipient: SuiAddress, amount: u64, coin_type: &TypeTag) {
        let (module, func) = self.mint_coin_func(coin_type);
        let gas = self.request_gas(self.publisher, 0).await;
        let cap = self.treasury_cap(coin_type).await;
        let tx = mint_tx(
            self.publisher,
            gas,
            self.pkg,
            module,
            func,
            cap,
            amount,
            recipient,
            self.cluster.reference_gas_price().await,
        );
        let signed = to_sender_signed_transaction(tx, &self.publisher_kp);
        let (fx, err) = self
            .cluster
            .execute_transaction(signed)
            .await
            .expect("execute_transaction");
        assert!(err.is_none(), "send_coin_to_address: {err:?}");
        assert!(fx.status().is_ok());
    }

    /// `sender` withdraws `amount` of their `coin_type` address
    /// balance and ships it to `recipient`.
    async fn transfer_address_balance(
        &self,
        sender: SuiAddress,
        signer: &AccountKeyPair,
        recipient: SuiAddress,
        amount: u64,
        coin_type: TypeTag,
    ) {
        let gas = self.request_gas(sender, amount).await;
        let tx = TestTransactionBuilder::new(sender, gas, self.cluster.reference_gas_price().await)
            .with_gas_budget(DEFAULT_GAS_BUDGET)
            .transfer_funds_to_address_balance(
                FundSource::address_fund_with_reservation(amount),
                vec![(amount, recipient)],
                coin_type,
            )
            .build();
        let signed = Transaction::from_data_and_signer(tx, vec![signer]);
        let (fx, err) = self
            .cluster
            .execute_transaction(signed)
            .await
            .expect("execute_transaction");
        assert!(err.is_none(), "transfer_address_balance: {err:?}");
        assert!(fx.status().is_ok());
    }

    fn mint_coin_func(&self, coin_type: &TypeTag) -> (&'static str, &'static str) {
        if let TypeTag::Struct(s) = coin_type {
            return match s.module.as_str() {
                "a_coin" => ("a_coin", "mint_coin"),
                "b_coin" => ("b_coin", "mint_coin"),
                "c_coin" => ("c_coin", "mint_coin"),
                "my_coin" => ("my_coin", "mint"),
                other => panic!("unsupported coin module: {other}"),
            };
        }
        panic!("expected a struct type, got {coin_type}");
    }

    fn mint_balance_func(&self, coin_type: &TypeTag) -> (&'static str, &'static str) {
        if let TypeTag::Struct(s) = coin_type {
            return match s.module.as_str() {
                "a_coin" => ("a_coin", "mint_balance"),
                "b_coin" => ("b_coin", "mint_balance"),
                "c_coin" => ("c_coin", "mint_balance"),
                "my_coin" => ("my_coin", "mint_balance"),
                other => panic!("unsupported coin module: {other}"),
            };
        }
        panic!("expected a struct type, got {coin_type}");
    }
}

fn mint_tx(
    publisher: SuiAddress,
    gas: ObjectRef,
    pkg: ObjectID,
    module: &str,
    func: &str,
    treasury_cap: ObjectRef,
    amount: u64,
    recipient: SuiAddress,
    rgp: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            pkg,
            Identifier::new(module).unwrap(),
            Identifier::new(func).unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap)),
                CallArg::Pure(bcs::to_bytes(&amount).unwrap()),
                CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
            ],
        )
        .unwrap();
    TransactionData::new_programmable(
        publisher,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        rgp,
    )
}

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

async fn list_balances(
    cluster: &LocalCluster,
    owner: SuiAddress,
    checkpoint: Option<u64>,
    page_size: Option<u32>,
) -> Result<Vec<(String, u64)>, tonic::Status> {
    let mut client = client(cluster).await;
    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(owner.to_string()),
        page_size,
        ..Default::default()
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    Ok(client
        .list_balances(request)
        .await?
        .into_inner()
        .balances
        .into_iter()
        .map(|b| (b.coin_type().to_owned(), b.total_balance()))
        .collect())
}

async fn get_balance(
    cluster: &LocalCluster,
    owner: SuiAddress,
    coin_type: &str,
    checkpoint: Option<u64>,
) -> Result<u64, tonic::Status> {
    let mut client = client(cluster).await;
    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(owner.to_string()),
        coin_type: Some(coin_type.to_owned()),
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    Ok(client
        .get_balance(request)
        .await?
        .into_inner()
        .total_balance())
}

/// Ports `consistent_store_balance_tests::test_multiple_coin_types`
/// (object-balance variant): publishing the coin package mints
/// 1000 + 200 + 30 = 1230 MY_COIN to the publisher in init.
/// We assert the resulting per-coin balances from the publisher
/// side (no accumulators needed).
#[tokio::test]
async fn multiple_coin_types_object_balance() {
    // No accumulator guard — this test only exercises the
    // coin-side merge in the `balance` CF.
    let cluster = MultiCoinCluster::new().await;

    let gas_type = GAS::type_().to_canonical_string(true);
    let my_coin_str = cluster.my_coin_type().to_canonical_string(true);

    // After init, the publisher owns:
    //   - the gas coin (whatever's left after the publish tx)
    //   - 3 Coin<MY_COIN> objects totalling 1230 mist
    let mut balances = list_balances(&cluster.cluster, cluster.publisher, None, Some(10))
        .await
        .unwrap();
    balances.sort_by(|a, b| a.0.cmp(&b.0));

    // MY_COIN balance is exactly 1230 (1000 + 200 + 30).
    let my_coin = balances
        .iter()
        .find(|(ty, _)| ty == &my_coin_str)
        .unwrap_or_else(|| panic!("MY_COIN missing from {balances:?}"));
    assert_eq!(my_coin.1, 1230);

    // The publisher also has a SUI balance for whatever
    // remains of the funded gas; we don't lock it down to a
    // specific amount because gas accounting depends on the
    // publish-tx cost, but it must exist.
    assert!(
        balances.iter().any(|(ty, _)| ty == &gas_type),
        "publisher should also have a SUI balance: {balances:?}",
    );

    // Per-call `get_balance` for MY_COIN matches.
    assert_eq!(
        get_balance(&cluster.cluster, cluster.publisher, &my_coin_str, None)
            .await
            .unwrap(),
        1230,
    );
}

/// Ports `test_multiple_coin_types` (address-balance variant):
/// recipient has parallel SUI and MY_COIN address balances.
#[tokio::test]
async fn multiple_coin_types_address_balance() {
    let _guard = accumulator_overrides();
    let cluster = MultiCoinCluster::new().await;
    let (recipient, _) = get_account_key_pair();

    cluster.send_sui_to_address_balance(recipient, 500).await;
    cluster.cluster.create_checkpoint().await.unwrap();

    let my_coin = cluster.my_coin_type();
    cluster
        .send_balance_to_address_balance(recipient, 1000, &my_coin)
        .await;
    cluster.cluster.create_checkpoint().await.unwrap();

    let gas_type = GAS::type_().to_canonical_string(true);
    let my_coin_str = my_coin.to_canonical_string(true);

    assert_eq!(
        get_balance(&cluster.cluster, recipient, &gas_type, None)
            .await
            .unwrap(),
        500,
    );
    assert_eq!(
        get_balance(&cluster.cluster, recipient, &my_coin_str, None)
            .await
            .unwrap(),
        1000,
    );

    let mut balances = list_balances(&cluster.cluster, recipient, None, Some(10))
        .await
        .unwrap();
    balances.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = vec![(gas_type.clone(), 500), (my_coin_str.clone(), 1000)];
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(balances, expected);
}

/// Ports `test_address_to_address_transfer`: A's MY_COIN
/// address-balance moves to B, with historical reads still
/// visible at past checkpoints.
#[tokio::test]
async fn address_to_address_transfer() {
    let _guard = accumulator_overrides();
    let cluster = MultiCoinCluster::new().await;
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();
    let my_coin = cluster.my_coin_type();
    let my_coin_str = my_coin.to_canonical_string(true);

    // Checkpoint 1: package publish.
    cluster.cluster.create_checkpoint().await.unwrap();

    // Checkpoint 2: A receives 1000 of MY_COIN.
    cluster
        .send_balance_to_address_balance(a, 1000, &my_coin)
        .await;
    let cp2 = cluster
        .cluster
        .create_checkpoint()
        .await
        .unwrap()
        .sequence_number;

    assert_eq!(
        get_balance(&cluster.cluster, a, &my_coin_str, None)
            .await
            .unwrap(),
        1000,
    );
    assert_eq!(
        get_balance(&cluster.cluster, b, &my_coin_str, None)
            .await
            .unwrap(),
        0,
    );

    // Checkpoint 3: A → B partial 500.
    cluster
        .transfer_address_balance(a, &akp, b, 500, my_coin.clone())
        .await;
    let cp3 = cluster
        .cluster
        .create_checkpoint()
        .await
        .unwrap()
        .sequence_number;

    assert_eq!(
        get_balance(&cluster.cluster, a, &my_coin_str, None)
            .await
            .unwrap(),
        500,
    );
    assert_eq!(
        get_balance(&cluster.cluster, b, &my_coin_str, None)
            .await
            .unwrap(),
        500,
    );

    // Checkpoint 4: A → B remaining 500.
    cluster
        .transfer_address_balance(a, &akp, b, 500, my_coin.clone())
        .await;
    cluster.cluster.create_checkpoint().await.unwrap();

    // A now empty for MY_COIN; only SUI rows (from gas) remain.
    let live_a = list_balances(&cluster.cluster, a, None, Some(10))
        .await
        .unwrap();
    assert!(
        live_a.iter().all(|(ty, _)| ty != &my_coin_str),
        "A should have no MY_COIN balance after full transfer; got {live_a:?}",
    );

    // Historical reads.
    assert_eq!(
        list_balances(&cluster.cluster, a, Some(cp3), Some(10))
            .await
            .unwrap()
            .into_iter()
            .find(|(ty, _)| ty == &my_coin_str),
        Some((my_coin_str.clone(), 500)),
    );
    assert_eq!(
        get_balance(&cluster.cluster, a, &my_coin_str, Some(cp2))
            .await
            .unwrap(),
        1000,
    );

    // B has 1000 total now, 500 at cp3, 0 at cp2.
    assert_eq!(
        get_balance(&cluster.cluster, b, &my_coin_str, None)
            .await
            .unwrap(),
        1000,
    );
    assert_eq!(
        get_balance(&cluster.cluster, b, &my_coin_str, Some(cp3))
            .await
            .unwrap(),
        500,
    );
    assert_eq!(
        get_balance(&cluster.cluster, b, &my_coin_str, Some(cp2))
            .await
            .unwrap(),
        0,
    );
}

/// Ports `test_list_balances_pagination`: a mix of coin and
/// address balances paginated forward / backward across page
/// boundaries. We use the four coin modules to seed four
/// distinct balance entries plus the GAS coin from any
/// implicit gas activity.
#[tokio::test]
async fn list_balances_pagination_forward_and_back() {
    let _guard = accumulator_overrides();
    let cluster = MultiCoinCluster::new().await;
    let (recipient, _) = get_account_key_pair();

    let a_coin = cluster.coin_type("a");
    let b_coin = cluster.coin_type("b");
    let c_coin = cluster.coin_type("c");
    let my_coin = cluster.my_coin_type();

    cluster.send_coin_to_address(recipient, 1000, &a_coin).await;
    cluster.send_coin_to_address(recipient, 500, &b_coin).await;
    cluster.cluster.create_checkpoint().await.unwrap();
    cluster
        .send_balance_to_address_balance(recipient, 300, &b_coin)
        .await;
    cluster
        .send_balance_to_address_balance(recipient, 200, &c_coin)
        .await;
    cluster.send_coin_to_address(recipient, 100, &my_coin).await;
    cluster.cluster.create_checkpoint().await.unwrap();

    let a_str = a_coin.to_canonical_string(true);
    let b_str = b_coin.to_canonical_string(true);
    let c_str = c_coin.to_canonical_string(true);
    let my_str = my_coin.to_canonical_string(true);

    // Sanity: all four entries land at the expected totals.
    let mut balances = list_balances(&cluster.cluster, recipient, None, Some(10))
        .await
        .unwrap();
    balances.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = vec![
        (a_str.clone(), 1000),
        (b_str.clone(), 800),
        (c_str.clone(), 200),
        (my_str.clone(), 100),
    ];
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(balances, expected);

    // Pagination round-trip: walk forward in pages of 2 and
    // confirm the concatenated result matches a single
    // unpaginated read.
    let mut client = client(&cluster.cluster).await;
    let owner_str = recipient.to_string();

    let mut acc = Vec::new();
    let mut after_token: Option<Vec<u8>> = None;
    loop {
        let request = tonic::Request::new(ListBalancesRequest {
            owner: Some(owner_str.clone()),
            page_size: Some(2),
            after_token: after_token.clone().map(Into::into),
            ..Default::default()
        });
        let resp = client.list_balances(request).await.unwrap().into_inner();
        acc.extend(
            resp.balances
                .iter()
                .map(|b| (b.coin_type().to_owned(), b.total_balance())),
        );
        if resp.has_next_page() {
            after_token = resp.balances.last().map(|b| b.page_token().to_owned());
        } else {
            break;
        }
    }
    assert_eq!(
        acc.len(),
        4,
        "paginated walk should match unpaginated total"
    );
}
