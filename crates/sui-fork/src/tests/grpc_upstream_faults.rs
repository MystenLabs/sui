// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Upstream GraphQL faults on the real gRPC SimulateTransaction / ExecuteTransaction path, against
//! a scripted upstream (`crate::upstream_mock`). A fault that a resend absorbs must leave simulate
//! succeeding. A read that fails on every attempt panics before anything is committed, rather
//! than let execute commit the VM's `VMInvariantViolation`.

use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use rand::rngs::OsRng;
use sui_rpc_api::proto::sui::rpc::v2 as proto;
use sui_rpc_api::proto::sui::rpc::v2::simulate_transaction_request::TransactionChecks as ProtoTransactionChecks;
use sui_rpc_api::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_swarm_config::genesis_config::DEFAULT_GAS_AMOUNT;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::KeypairTraits;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::ObjectStore;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionKind;

use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::store::ForkStore;
use crate::test_support::ForkServer;
use crate::upstream_mock::ATTEMPTS_PER_OBJECT_READ;
use crate::upstream_mock::ReadKind;
use crate::upstream_mock::UpstreamScript;
use crate::upstream_mock::html_page;
use crate::upstream_mock::u64_field_id;

/// Deadline for every gRPC call, so a hung request fails the test instead of stalling it.
const RPC_TIMEOUT: Duration = Duration::from_secs(60);

/// Budget of the setup transaction that creates and shares the Bag.
const SETUP_BUDGET: u64 = 50_000_000;

/// Dynamic-field key the probe checks with `bag::contains<u64>`.
const PROBED_KEY: u64 = 7;

/// A fork over a scripted upstream, whose genesis gives the sender a single gas coin.
struct FaultHarness {
    fork: ForkServer,
    script: UpstreamScript,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_coin: Object,
}

impl FaultHarness {
    async fn start() -> Result<Self> {
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut OsRng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .with_accounts(vec![AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT],
            }])
            .build();
        let sender_key = config
            .account_keys
            .first()
            .ok_or_else(|| anyhow!("genesis config has no account keys"))?
            .copy();
        let sender: SuiAddress = sender_key.public().into();
        let gas_coin = config
            .genesis
            .objects()
            .iter()
            .find(|obj| obj.owner == Owner::AddressOwner(sender) && obj.is_gas_coin())
            .cloned()
            .ok_or_else(|| anyhow!("sender should own a genesis gas coin"))?;

        let script = UpstreamScript::start().await;
        let fork = ForkServer::start(&config, script.uri()).await?;
        Ok(Self {
            fork,
            script,
            sender,
            sender_key,
            gas_coin,
        })
    }

    async fn execution_client(
        &self,
    ) -> Result<TransactionExecutionServiceClient<tonic::transport::Channel>> {
        Ok(TransactionExecutionServiceClient::connect(self.fork.grpc_endpoint.clone()).await?)
    }

    async fn latest_checkpoint(&self) -> Result<u64> {
        let mut client = ForkingServiceClient::connect(self.fork.grpc_endpoint.clone()).await?;
        let status = tokio::time::timeout(RPC_TIMEOUT, client.get_status(GetStatusRequest {}))
            .await
            .map_err(|_| anyhow!("get_status timed out"))??
            .into_inner();
        Ok(status.checkpoint_sequence_number)
    }

    fn local_version(&self, id: ObjectID) -> Option<u64> {
        <ForkStore as ObjectStore>::get_object(&self.fork.store, &id).map(|o| o.version().value())
    }

    /// Sign `data` with the sender key and execute it over gRPC. Returns the effects, or the gRPC
    /// status the fork answered with; a call that gets no answer within [`RPC_TIMEOUT`] is an
    /// error.
    async fn execute_signed(
        &self,
        data: &TransactionData,
    ) -> Result<std::result::Result<TransactionEffects, tonic::Status>> {
        let signed = Transaction::from_data_and_signer(data.clone(), vec![&self.sender_key]);
        let signatures = signed
            .data()
            .tx_signatures()
            .iter()
            .map(|signature| {
                let mut message = proto::UserSignature::default();
                message.bcs = Some(signature.as_ref().to_vec().into());
                message
            })
            .collect();
        let mut transaction = proto::Transaction::default();
        transaction.bcs = Some(proto::Bcs::serialize(data)?);
        let mut request = proto::ExecuteTransactionRequest::new(transaction);
        request.signatures = signatures;
        let mut mask = request.read_mask.take().unwrap_or_default();
        mask.paths = vec!["effects.bcs".to_owned(), "effects.status".to_owned()];
        request.read_mask = Some(mask);

        let mut client = self.execution_client().await?;
        let response =
            match tokio::time::timeout(RPC_TIMEOUT, client.execute_transaction(request)).await {
                Err(_) => bail!("execute timed out"),
                Ok(Err(status)) => return Ok(Err(status)),
                Ok(Ok(response)) => response.into_inner(),
            };
        let effects_bcs = response
            .transaction
            .and_then(|t| t.effects)
            .and_then(|e| e.bcs)
            .ok_or_else(|| anyhow!("execute response has no effects.bcs"))?;
        Ok(Ok(effects_bcs.deserialize::<TransactionEffects>()?))
    }

    /// Create a Bag and share it, paying with the sender's single genesis coin. This also gets the
    /// coin owner-indexed, which gas selection needs. Returns the Bag id.
    async fn create_shared_bag(&self) -> Result<ObjectID> {
        let mut builder = ProgrammableTransactionBuilder::new();
        let bag = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("bag")?,
            Identifier::new("new")?,
            vec![],
            vec![],
        );
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("transfer")?,
            Identifier::new("public_share_object")?,
            vec![bag_type_tag()],
            vec![bag],
        );
        let data = TransactionData::new_with_gas_data(
            TransactionKind::ProgrammableTransaction(builder.finish()),
            self.sender,
            GasData {
                payment: vec![self.gas_coin.compute_object_reference()],
                owner: self.sender,
                price: self.fork.reference_gas_price,
                budget: SETUP_BUDGET,
            },
        );
        let effects = self
            .execute_signed(&data)
            .await?
            .map_err(|status| anyhow!("setup transaction failed at the RPC layer: {status:?}"))?;
        if !effects.status().is_ok() {
            bail!("setup transaction failed: {:?}", effects.status());
        }
        effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
            .map(|((id, _, _), _)| id)
            .ok_or_else(|| anyhow!("setup effects created no shared object"))
    }
}

/// A fork holding a shared Bag whose dynamic-field child for `PROBED_KEY` reads upstream, absent
/// unless scripted otherwise.
struct BagProbe {
    harness: FaultHarness,
    bag: ObjectID,
    child: ObjectID,
}

impl BagProbe {
    async fn start() -> Self {
        let harness = FaultHarness::start().await.expect("harness should start");
        let bag = harness
            .create_shared_bag()
            .await
            .expect("setup should create a shared Bag");
        Self {
            harness,
            bag,
            child: u64_field_id(bag, PROBED_KEY),
        }
    }

    /// Script the upstream's next replies to reads of the probed child.
    fn script_child(&self, replies: Vec<crate::upstream_mock::UpstreamReply>) {
        self.harness
            .script
            .script(ReadKind::Child, self.child, replies);
    }

    /// Simulate the probe the way the TypeScript SDK builds a transaction: unresolved, with no gas
    /// payment or budget, and with gas selection, so simulate runs an estimation pass and a final
    /// pass that each read the child. Returns the effects and the transaction simulate resolved.
    async fn simulate(
        &self,
    ) -> std::result::Result<(TransactionEffects, TransactionData), tonic::Status> {
        let mut request = proto::SimulateTransactionRequest::new(probe_transaction(
            self.harness.sender,
            self.bag,
        ))
        .with_do_gas_selection(true);
        request.set_checks(ProtoTransactionChecks::Enabled);
        let mut mask = request.read_mask.take().unwrap_or_default();
        mask.paths = vec![
            "transaction.effects.bcs".to_owned(),
            "transaction.transaction.bcs".to_owned(),
        ];
        request.read_mask = Some(mask);

        let mut client = self
            .harness
            .execution_client()
            .await
            .expect("client should connect");
        let executed = tokio::time::timeout(RPC_TIMEOUT, client.simulate_transaction(request))
            .await
            .expect("simulate should not time out")?
            .into_inner()
            .transaction
            .expect("simulate response should carry the transaction");
        let effects = executed
            .effects
            .and_then(|effects| effects.bcs)
            .expect("simulate response should carry effects.bcs")
            .deserialize()
            .expect("effects should decode");
        let data = executed
            .transaction
            .and_then(|transaction| transaction.bcs)
            .expect("simulate response should carry transaction.bcs")
            .deserialize()
            .expect("transaction should decode");
        Ok((effects, data))
    }

    fn child_requests(&self) -> usize {
        self.harness.script.count(ReadKind::Child, self.child)
    }
}

fn bag_type_tag() -> TypeTag {
    TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: Identifier::new("bag").expect("valid identifier"),
        name: Identifier::new("Bag").expect("valid identifier"),
        type_params: vec![],
    }))
}

/// Unresolved, TypeScript-SDK-shaped probe: `bag::contains<u64>(&bag, PROBED_KEY)`. No BCS, gas
/// payment, price or budget.
fn probe_transaction(sender: SuiAddress, bag: ObjectID) -> proto::Transaction {
    let mut bag_input = proto::Input::default();
    bag_input.object_id = Some(bag.to_canonical_string(true));
    let mut key_input = proto::Input::default();
    key_input.pure = Some(
        bcs::to_bytes(&PROBED_KEY)
            .expect("pure input should serialize")
            .into(),
    );

    let mut call = proto::MoveCall::default();
    call.package = Some(SUI_FRAMEWORK_PACKAGE_ID.to_canonical_string(true));
    call.module = Some("bag".to_owned());
    call.function = Some("contains".to_owned());
    call.type_arguments = vec!["u64".to_owned()];
    call.arguments = vec![proto::Argument::new_input(0), proto::Argument::new_input(1)];

    let mut ptb = proto::ProgrammableTransaction::default();
    ptb.inputs = vec![bag_input, key_input];
    ptb.commands = vec![proto::Command::from(call)];

    let mut transaction = proto::Transaction::default();
    transaction.kind = Some(proto::TransactionKind::from(ptb));
    transaction.sender = Some(sender.to_string());
    transaction
}

/// A child read that fails once is sent again, so simulate succeeds as it does without the
/// fault, with exactly one extra request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simulate_resends_a_failed_child_read() {
    let probe = BagProbe::start().await;
    let (clean_effects, _) = probe
        .simulate()
        .await
        .expect("fault-free simulate should succeed");
    let clean_requests = probe.child_requests();

    probe.script_child(vec![html_page(502)]);
    let (resent_effects, _) = probe
        .simulate()
        .await
        .expect("simulate with one failed response should succeed");

    assert!(clean_effects.status().is_ok(), "{clean_effects:?}");
    assert!(resent_effects.status().is_ok(), "{resent_effects:?}");
    assert_eq!(probe.child_requests(), 2 * clean_requests + 1);
}

/// A dynamic-field read that fails on every attempt during execute panics before the transaction
/// is committed: no checkpoint is created and the gas coin keeps its version.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn execute_commits_nothing_when_a_child_read_fails_every_attempt() {
    let probe = BagProbe::start().await;
    let (_, data) = probe
        .simulate()
        .await
        .expect("simulate should succeed while the child reads as absent");
    let gas = *data
        .gas_data()
        .payment
        .first()
        .expect("simulate should select a gas coin");
    let checkpoint_before = probe
        .harness
        .latest_checkpoint()
        .await
        .expect("status should be readable");
    let requests_before = probe.child_requests();

    probe.script_child(vec![html_page(502); ATTEMPTS_PER_OBJECT_READ]);
    probe
        .harness
        .execute_signed(&data)
        .await
        .expect("execute should answer")
        .expect_err("execute must fail when the child cannot be read");

    assert_eq!(
        probe.child_requests() - requests_before,
        ATTEMPTS_PER_OBJECT_READ
    );
    assert_eq!(
        probe
            .harness
            .latest_checkpoint()
            .await
            .expect("status should be readable"),
        checkpoint_before
    );
    assert_eq!(probe.harness.local_version(gas.0), Some(gas.1.value()));
}
