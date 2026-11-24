// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::authority_test_utils::{submit_and_execute_with_options, vote_transaction};
use crate::authority::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use move_core_types::ident_str;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin_regulation::{
    CoinTypeKey, CoinTypeState, FreezeListField, FrozenAddressKey, FrozenAddressState, UnfreezeLock,
};
use sui_types::collection_types::VecSet;
use sui_types::crypto::{AccountKeyPair, get_account_key_pair};
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiError, SuiErrorKind, UserInputError};
use sui_types::object::{MoveObject, OBJECT_START_VERSION, Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, FundsWithdrawalArg, ObjectArg, SharedObjectMutability,
    TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_PUBLISH, Transaction, TransactionData,
};
use sui_types::{
    MoveTypeTagTrait, SUI_COIN_REGISTRY_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID,
    SUI_FRAMEWORK_PACKAGE_ID, SUI_FREEZE_LIST_OBJECT_ID,
};

struct TestEnv {
    authority: Arc<AuthorityState>,
    issuer: SuiAddress,
    issuer_key: AccountKeyPair,
    issuer_gas: ObjectID,
    user: SuiAddress,
    user_key: AccountKeyPair,
    package_id: ObjectID,
    /// The `DenyCapV2`, and after migration the `ComplianceCap` with the same ID.
    cap: ObjectID,
    metadata: ObjectID,
    coins: Vec<ObjectID>,
    plain_coin: ObjectID,
}

impl TestEnv {
    /// Publishes a regulated coin and an unregulated coin owned by the issuer, and migrates the
    /// regulated coin to the FreezeList when `enable_coin_regulation` is set.
    async fn new(enable_coin_regulation: bool) -> Self {
        Self::with_max_address_reads(enable_coin_regulation, 10_000).await
    }

    async fn with_max_address_reads(enable_coin_regulation: bool, max_reads: u64) -> Self {
        let (issuer, issuer_key) = get_account_key_pair();
        let (user, user_key) = get_account_key_pair();
        let issuer_gas = Object::with_owner_for_testing(issuer);
        let issuer_gas_id = issuer_gas.id();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_enable_accumulators_for_testing(true);
        protocol_config.set_enable_coin_regulation_for_testing(enable_coin_regulation);
        protocol_config.set_max_freeze_list_address_reads_for_testing(max_reads);

        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(&[issuer_gas])
            .with_protocol_config(protocol_config)
            .build()
            .await;
        let rgp = authority.reference_gas_price_for_testing().unwrap();
        let (_, effects) = build_and_try_publish_test_package(
            &authority,
            &issuer,
            &issuer_key,
            &issuer_gas_id,
            "coin_regulation",
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
            false,
        )
        .await;
        let effects = effects.into_data();
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        authority
            .settle_accumulator_for_testing(std::slice::from_ref(&effects), None)
            .await;

        let mut package_id = None;
        let mut cap = None;
        let mut metadata = None;
        let mut coins = vec![];
        let mut plain_coin = None;
        for (oref, _) in effects.created() {
            let object = authority.get_object(&oref.0).unwrap();
            if object.is_package() {
                package_id = Some(object.id());
                continue;
            }
            let type_ = object.type_().unwrap();
            if type_.is_coin_deny_cap_v2() {
                cap = Some(object.id());
            } else if type_.is_coin_metadata()
                && matches!(
                    &object.struct_tag().unwrap().type_params[0],
                    TypeTag::Struct(t) if t.name.as_str() == "REGULATED_COIN"
                )
            {
                metadata = Some(object.id());
            } else if let Some(TypeTag::Struct(coin_type)) = object.coin_type_maybe() {
                match coin_type.name.as_str() {
                    "REGULATED_COIN" => coins.push(object.id()),
                    "PLAIN_COIN" => plain_coin = Some(object.id()),
                    _ => (),
                }
            }
        }
        assert_eq!(coins.len(), 2);

        let env = Self {
            authority,
            issuer,
            issuer_key,
            issuer_gas: issuer_gas_id,
            user,
            user_key,
            package_id: package_id.unwrap(),
            cap: cap.unwrap(),
            metadata: metadata.unwrap(),
            coins,
            plain_coin: plain_coin.unwrap(),
        };
        if enable_coin_regulation {
            env.migrate().await;
        }
        env
    }

    fn coin_type(&self) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: self.package_id.into(),
            module: ident_str!("regulated_coin").to_owned(),
            name: ident_str!("REGULATED_COIN").to_owned(),
            type_params: vec![],
        }))
    }

    fn coin_type_name(&self) -> String {
        self.coin_type().to_canonical_string(false)
    }

    fn object_ref(&self, id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(id)
            .unwrap()
            .compute_object_reference()
    }

    fn system_object_arg(&self, id: ObjectID) -> ObjectArg {
        let Owner::Shared {
            initial_shared_version,
        } = self.authority.get_object(&id).unwrap().owner
        else {
            panic!("{id} is not shared");
        };
        ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        }
    }

    fn issuer_tx(&self, ptb: ProgrammableTransactionBuilder) -> Transaction {
        let rgp = self.authority.reference_gas_price_for_testing().unwrap();
        let data = TransactionData::new_programmable(
            self.issuer,
            vec![self.object_ref(&self.issuer_gas)],
            ptb.finish(),
            TEST_ONLY_GAS_UNIT_FOR_GENERIC * rgp,
            rgp,
        );
        Transaction::from_data_and_signer(data, vec![&self.issuer_key])
    }

    async fn execute(&self, tx: Transaction) -> TransactionEffects {
        let (_, effects) = submit_and_execute_with_options(&self.authority, None, tx)
            .await
            .unwrap();
        let effects = effects.into_data();
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        effects
    }

    fn regulation_call(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        function: &str,
        mut args: Vec<Argument>,
    ) -> Argument {
        let freeze_list = ptb
            .obj(self.system_object_arg(SUI_FREEZE_LIST_OBJECT_ID))
            .unwrap();
        let cap = ptb
            .obj(ObjectArg::ImmOrOwnedObject(self.object_ref(&self.cap)))
            .unwrap();
        args.splice(0..0, [freeze_list, cap]);
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_regulation").to_owned(),
            Identifier::new(function).unwrap(),
            vec![self.coin_type()],
            args,
        )
    }

    /// Registers the legacy coin's `Currency` and migrates its `DenyCapV2` through it.
    async fn migrate(&self) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let registry = ptb
            .obj(self.system_object_arg(SUI_COIN_REGISTRY_OBJECT_ID))
            .unwrap();
        let metadata = ptb
            .obj(ObjectArg::ImmOrOwnedObject(self.object_ref(&self.metadata)))
            .unwrap();
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_registry").to_owned(),
            ident_str!("migrate_legacy_metadata").to_owned(),
            vec![self.coin_type()],
            vec![registry, metadata],
        );
        let effects = self.execute(self.issuer_tx(ptb)).await;
        let (currency, _) = effects
            .created()
            .into_iter()
            .find(|(_, owner)| owner.is_shared())
            .unwrap();

        let mut ptb = ProgrammableTransactionBuilder::new();
        let currency = ptb.obj(self.system_object_arg(currency.0)).unwrap();
        let freeze_list = ptb
            .obj(self.system_object_arg(SUI_FREEZE_LIST_OBJECT_ID))
            .unwrap();
        let deny_list = ptb
            .obj(self.system_object_arg(SUI_DENY_LIST_OBJECT_ID))
            .unwrap();
        let cap = ptb
            .obj(ObjectArg::ImmOrOwnedObject(self.object_ref(&self.cap)))
            .unwrap();
        let addresses = ptb.pure(Vec::<SuiAddress>::new()).unwrap();
        let compliance_cap = ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_registry").to_owned(),
            ident_str!("migrate_to_compliance_cap").to_owned(),
            vec![self.coin_type()],
            vec![currency, freeze_list, deny_list, cap, addresses],
        );
        ptb.transfer_arg(self.issuer, compliance_cap);
        self.execute(self.issuer_tx(ptb)).await;
    }

    async fn freeze(&self, addr: SuiAddress) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let addr = ptb.pure(addr).unwrap();
        self.regulation_call(&mut ptb, "freeze_address", vec![addr]);
        self.execute(self.issuer_tx(ptb)).await;
    }

    async fn pause(&self) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.regulation_call(&mut ptb, "enable_global_pause", vec![]);
        self.execute(self.issuer_tx(ptb)).await;
    }

    /// Issues a `SeizeAuth` for `frozen` to the issuer.
    async fn issue_seize_auth(&self, frozen: SuiAddress) -> ObjectID {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let frozen = ptb.pure(frozen).unwrap();
        let auth = self.regulation_call(&mut ptb, "issue_seize_auth", vec![frozen]);
        let recipient = ptb.pure(self.issuer).unwrap();
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_regulation").to_owned(),
            ident_str!("transfer_seize_auth").to_owned(),
            vec![self.coin_type()],
            vec![auth, recipient],
        );
        let effects = self.execute(self.issuer_tx(ptb)).await;
        effects
            .created()
            .into_iter()
            .map(|(oref, _)| oref.0)
            .find(|id| {
                let object = self.authority.get_object(id).unwrap();
                object
                    .struct_tag()
                    .is_some_and(|tag| tag.name.as_str() == "SeizeAuth")
            })
            .unwrap()
    }

    /// The issuer transfers `object` to `recipient`, with `extra` as additional unused inputs.
    fn transfer_tx(
        &self,
        object: ObjectID,
        recipient: SuiAddress,
        extra: &[ObjectID],
    ) -> Transaction {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let object = ptb
            .obj(ObjectArg::ImmOrOwnedObject(self.object_ref(&object)))
            .unwrap();
        for id in extra {
            ptb.obj(ObjectArg::ImmOrOwnedObject(self.object_ref(id)))
                .unwrap();
        }
        ptb.transfer_arg(recipient, object);
        self.issuer_tx(ptb)
    }
}

fn expect_user_input_error(result: Result<impl std::fmt::Debug, SuiError>) -> UserInputError {
    match result.expect_err("signing should fail").into_inner() {
        SuiErrorKind::UserInputError { error } => error,
        other => panic!("unexpected error: {other:?}"),
    }
}

fn assert_denied(env: &TestEnv, tx: Transaction, address: SuiAddress) {
    assert_eq!(
        expect_user_input_error(vote_transaction(&env.authority, tx)),
        UserInputError::AddressDeniedForCoin {
            address,
            coin_type: env.coin_type_name(),
        }
    );
}

#[tokio::test]
async fn test_frozen_sender_rejected() {
    let env = TestEnv::new(true).await;
    vote_transaction(&env.authority, env.transfer_tx(env.coins[0], env.user, &[])).unwrap();
    env.freeze(env.issuer).await;
    assert_denied(
        &env,
        env.transfer_tx(env.coins[0], env.user, &[]),
        env.issuer,
    );
}

#[tokio::test]
async fn test_address_read_limit_rejected() {
    let env = TestEnv::with_max_address_reads(true, 1).await;
    env.freeze(env.issuer).await;
    assert_eq!(
        expect_user_input_error(vote_transaction(
            &env.authority,
            env.transfer_tx(env.coins[0], env.user, &[]),
        )),
        UserInputError::SizeLimitExceeded {
            limit: "maximum FreezeList address reads in a transaction".to_string(),
            value: "1".to_string(),
        }
    );
}

#[tokio::test]
async fn test_frozen_sponsor_rejected() {
    let env = TestEnv::new(true).await;
    env.execute(env.transfer_tx(env.coins[0], env.user, &[]))
        .await;
    env.freeze(env.issuer).await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let coin = ptb
        .obj(ObjectArg::ImmOrOwnedObject(env.object_ref(&env.coins[0])))
        .unwrap();
    ptb.transfer_arg(env.user, coin);
    let rgp = env.authority.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_programmable_allow_sponsor(
        env.user,
        vec![env.object_ref(&env.issuer_gas)],
        ptb.finish(),
        TEST_ONLY_GAS_UNIT_FOR_GENERIC * rgp,
        rgp,
        env.issuer,
    );
    let tx = Transaction::from_data_and_signer(data, vec![&env.user_key, &env.issuer_key]);
    assert_denied(&env, tx, env.issuer);
}

#[tokio::test]
async fn test_frozen_coin_id_rejected() {
    let env = TestEnv::new(true).await;
    env.freeze(env.coins[0].into()).await;
    assert_denied(
        &env,
        env.transfer_tx(env.coins[0], env.user, &[]),
        env.coins[0].into(),
    );
    vote_transaction(&env.authority, env.transfer_tx(env.coins[1], env.user, &[])).unwrap();
}

#[tokio::test]
async fn test_frozen_coin_owner_rejected() {
    let env = TestEnv::new(true).await;
    let parent: SuiAddress = env.cap.into();
    env.execute(env.transfer_tx(env.coins[0], parent, &[]))
        .await;
    env.freeze(parent).await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let cap = ptb
        .obj(ObjectArg::ImmOrOwnedObject(env.object_ref(&env.cap)))
        .unwrap();
    ptb.obj(ObjectArg::Receiving(env.object_ref(&env.coins[0])))
        .unwrap();
    ptb.transfer_arg(env.issuer, cap);
    assert_denied(&env, env.issuer_tx(ptb), parent);
}

#[tokio::test]
async fn test_frozen_withdrawal_funder_rejected() {
    let env = TestEnv::new(true).await;
    env.freeze(env.issuer).await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let withdrawal = ptb
        .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(1, env.coin_type()))
        .unwrap();
    let balance = ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("balance").to_owned(),
        ident_str!("redeem_funds").to_owned(),
        vec![env.coin_type()],
        vec![withdrawal],
    );
    let recipient = ptb.pure(env.user).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("balance").to_owned(),
        ident_str!("send_funds").to_owned(),
        vec![env.coin_type()],
        vec![balance, recipient],
    );
    assert_denied(&env, env.issuer_tx(ptb), env.issuer);
}

#[tokio::test]
async fn test_paused_coin_rejected() {
    let env = TestEnv::new(true).await;
    env.pause().await;
    assert_eq!(
        expect_user_input_error(vote_transaction(
            &env.authority,
            env.transfer_tx(env.coins[0], env.user, &[]),
        )),
        UserInputError::CoinTypeGlobalPause {
            coin_type: env.coin_type_name(),
        }
    );
}

#[tokio::test]
async fn test_seize_auth_does_not_exempt_frozen_address() {
    let env = TestEnv::new(true).await;
    env.freeze(env.issuer).await;
    let seize_auth = env.issue_seize_auth(env.issuer).await;
    assert_denied(
        &env,
        env.transfer_tx(env.coins[0], env.user, &[seize_auth]),
        env.issuer,
    );
}

#[tokio::test]
async fn test_unregulated_coin_unaffected() {
    let env = TestEnv::new(true).await;
    env.freeze(env.issuer).await;
    vote_transaction(
        &env.authority,
        env.transfer_tx(env.plain_coin, env.user, &[]),
    )
    .unwrap();
}

fn freeze_list_field<K, V>(field: FreezeListField<K>, value: V) -> Object
where
    K: Serialize + DeserializeOwned + std::fmt::Debug,
    V: MoveTypeTagTrait + Serialize + DeserializeOwned,
{
    let field = field
        .into_field(value)
        .unwrap()
        .into_move_object_unsafe_for_testing(OBJECT_START_VERSION)
        .unwrap();
    Object::new_move(
        field,
        Owner::ObjectOwner(SUI_FREEZE_LIST_OBJECT_ID.into()),
        TransactionDigest::genesis_marker(),
    )
}

/// A transfer of a `Coin<T>` from a sender that genesis FreezeList state marks frozen for `T`.
async fn frozen_sender_at_genesis(
    enable_coin_regulation: bool,
) -> (Arc<AuthorityState>, Transaction) {
    let (sender, key) = get_account_key_pair();
    let coin_type: TypeTag = "0x1::string::String".parse().unwrap();
    let gas = Object::with_owner_for_testing(sender);
    let coin = Object::new_move(
        MoveObject::new_coin(
            coin_type.clone(),
            OBJECT_START_VERSION,
            ObjectID::random(),
            1,
        ),
        Owner::AddressOwner(sender),
        TransactionDigest::genesis_marker(),
    );
    let coin_type_state = freeze_list_field(
        CoinTypeKey::field(&coin_type),
        CoinTypeState {
            paused: false,
            num_frozen_addresses: 1,
        },
    );
    let frozen_address_state = freeze_list_field(
        FrozenAddressKey::field(&coin_type, sender),
        FrozenAddressState {
            seize_auths: VecSet { contents: vec![] },
            frozen_epoch: 0,
            unfreeze_lock: UnfreezeLock::None,
        },
    );

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_enable_coin_regulation_for_testing(enable_coin_regulation);
    let authority = TestAuthorityBuilder::new()
        .with_starting_objects(&[
            gas.clone(),
            coin.clone(),
            coin_type_state,
            frozen_address_state,
        ])
        .with_protocol_config(protocol_config)
        .build()
        .await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(SuiAddress::ZERO, coin.compute_full_object_reference())
        .unwrap();
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_programmable(
        sender,
        vec![gas.compute_object_reference()],
        ptb.finish(),
        TEST_ONLY_GAS_UNIT_FOR_GENERIC * rgp,
        rgp,
    );
    (
        authority,
        Transaction::from_data_and_signer(data, vec![&key]),
    )
}

async fn vote_with_frozen_sender_at_genesis(enable_coin_regulation: bool) -> Result<(), SuiError> {
    let (authority, tx) = frozen_sender_at_genesis(enable_coin_regulation).await;
    vote_transaction(&authority, tx).map(|_| ())
}

#[tokio::test]
async fn test_coin_regulation_disabled_unaffected() {
    assert!(matches!(
        expect_user_input_error(vote_with_frozen_sender_at_genesis(true).await),
        UserInputError::AddressDeniedForCoin { .. }
    ));
    vote_with_frozen_sender_at_genesis(false).await.unwrap();
}

#[tokio::test]
async fn test_coin_regulation_disabled_execution_ignores_freeze_list() {
    let (authority, tx) = frozen_sender_at_genesis(false).await;
    let (_, effects) = submit_and_execute_with_options(&authority, None, tx)
        .await
        .unwrap();
    let effects = effects.into_data();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(
        effects
            .unchanged_consensus_objects()
            .iter()
            .all(|(id, _)| *id != SUI_FREEZE_LIST_OBJECT_ID)
    );
}
