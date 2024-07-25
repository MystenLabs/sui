// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;
use fastcrypto_zkp::bn254::zk_login::OIDCProvider;
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use move_core_types::language_storage::{StructTag, TypeTag};
use pretty_assertions::assert_str_eq;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use std::str::FromStr;
use std::{fs::File, io::Write};
use sui_types::execution_status::{
    CommandArgumentError, ExecutionFailureStatus, ExecutionStatus, PackageUpgradeError,
    TypeArgumentError,
};
use sui_types::messages_grpc::ObjectInfoRequestKind;
use sui_types::move_package::TypeOrigin;
use sui_types::{
    base_types::MoveObjectType_,
    crypto::Signer,
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSummary,
        FullCheckpointContents,
    },
    transaction::TransactionExpiration,
};
use sui_types::{
    base_types::{
        self, MoveObjectType, ObjectDigest, ObjectID, TransactionDigest, TransactionEffectsDigest,
    },
    crypto::{
        get_key_pair, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
        AuthorityPublicKeyBytes, AuthoritySignature, KeypairTraits, Signature, SuiKeyPair,
    },
    multisig::{MultiSig, MultiSigPublicKey},
    object::{Data, Owner},
    signature::GenericSignature,
    storage::DeleteKind,
    transaction::{
        Argument, CallArg, Command, EndOfEpochTransactionKind, ObjectArg, TransactionKind,
    },
};
use sui_types::{
    crypto::{PublicKey, ZkLoginPublicIdentifier},
    effects::{IDOperation, ObjectIn, ObjectOut, TransactionEffects, UnchangedSharedKind},
    utils::DEFAULT_ADDRESS_SEED,
};
use typed_store::TypedStoreError;
fn get_registry() -> Result<Registry> {
    let config = TracerConfig::default()
        .record_samples_for_structs(true)
        .record_samples_for_newtype_structs(true);
    let mut tracer = Tracer::new(config);
    let mut samples = Samples::new();
    // 1. Record samples for types with custom deserializers.
    // We want to call
    // tracer.trace_value(&mut samples, ...)?;
    // with all the base types contained in messages, especially the ones with custom serializers;
    // or involving generics (see [serde_reflection documentation](https://novifinancial.github.io/serde-reflection/serde_reflection/index.html)).
    let (addr, kp): (_, AuthorityKeyPair) = get_key_pair();
    let (s_addr, s_kp): (_, AccountKeyPair) = get_key_pair();
    let pk: AuthorityPublicKeyBytes = kp.public().into();
    tracer.trace_value(&mut samples, &addr)?;
    tracer.trace_value(&mut samples, &kp)?;
    tracer.trace_value(&mut samples, &pk)?;

    tracer.trace_value(&mut samples, &s_addr)?;
    tracer.trace_value(&mut samples, &s_kp)?;

    // We have two signature types: one for Authority Signatures, which don't include the PubKey ...
    let sig: AuthoritySignature = Signer::sign(&kp, b"hello world");
    tracer.trace_value(&mut samples, &sig)?;
    // ... and the user signature which does

    let sig: Signature = Signer::sign(&s_kp, b"hello world");
    tracer.trace_value(&mut samples, &sig)?;

    let kp1: SuiKeyPair =
        SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);
    let kp2: SuiKeyPair =
        SuiKeyPair::Secp256k1(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);
    let kp3: SuiKeyPair =
        SuiKeyPair::Secp256r1(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);
    let pk_zklogin = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &OIDCProvider::Twitch.get_config().iss,
            &Bn254FrElement::from_str(DEFAULT_ADDRESS_SEED).unwrap(),
        )
        .unwrap(),
    );

    let multisig_pk = MultiSigPublicKey::new(
        vec![kp1.public(), kp2.public(), kp3.public(), pk_zklogin],
        vec![1, 1, 1, 1],
        2,
    )
    .unwrap();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Message".as_bytes().to_vec(),
        },
    );

    let sig1: GenericSignature = Signature::new_secure(&msg, &kp1).into();
    let sig2: GenericSignature = Signature::new_secure(&msg, &kp2).into();
    let sig3: GenericSignature = Signature::new_secure(&msg, &kp3).into();
    let sig4: GenericSignature = GenericSignature::from_str("BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhAG6Bf8BLuaIEgvF8Lx2jVoRWKKRIlaLlEJxgvqwq5nDX+rvzJxYAUFd7KeQBd9upNx+CHpmINkfgj26jcHbbqAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==").unwrap();
    let sig5: GenericSignature = GenericSignature::from_str("BiVJlg3liA6MaHQ0Fw9kdmBbj+SuuaKGMseZXPO6gx2XYx0AAAAAigF7InR5cGUiOiJ3ZWJhdXRobi5nZXQiLCJjaGFsbGVuZ2UiOiJBQUFBdF9taklCMXZiVnBZTTZXVjZZX29peDZKOGFOXzlzYjhTS0ZidWtCZmlRdyIsIm9yaWdpbiI6Imh0dHA6Ly9sb2NhbGhvc3Q6NTE3MyIsImNyb3NzT3JpZ2luIjpmYWxzZX1iApjskL9Xyfopyg9Av7MSrcchSpfWqAYoJ+qfSId4gNmoQ1YNgj2alDpRIbq9kthmyGY25+k24FrW114PEoy5C+8DPRcOCTtACi3ZywtZ4UILhwV+Suh79rWtbKqDqhBQwxM=").unwrap();

    let multi_sig = MultiSig::combine(
        vec![sig1.clone(), sig2.clone(), sig3.clone(), sig4.clone()],
        multisig_pk,
    )
    .unwrap();
    tracer.trace_value(&mut samples, &multi_sig)?;

    let generic_sig_multi = GenericSignature::MultiSig(multi_sig);
    tracer.trace_value(&mut samples, &generic_sig_multi)?;

    tracer.trace_value(&mut samples, &sig1)?;
    tracer.trace_value(&mut samples, &sig2)?;
    tracer.trace_value(&mut samples, &sig3)?;
    tracer.trace_value(&mut samples, &sig4)?;
    tracer.trace_value(&mut samples, &sig5)?;
    // ObjectID and SuiAddress are the same length
    let oid: ObjectID = addr.into();
    tracer.trace_value(&mut samples, &oid)?;

    // ObjectDigest and Transaction digest use the `serde_as`speedup for ser/de => trace them
    let od = ObjectDigest::random();
    let td = TransactionDigest::random();
    tracer.trace_value(&mut samples, &od)?;
    tracer.trace_value(&mut samples, &td)?;

    let teff = TransactionEffectsDigest::random();
    tracer.trace_value(&mut samples, &teff)?;

    let ccd = CheckpointContentsDigest::random();
    tracer.trace_value(&mut samples, &ccd)?;

    let struct_tag = StructTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap();
    tracer.trace_value(&mut samples, &struct_tag)?;

    let ccd = CheckpointDigest::random();
    tracer.trace_value(&mut samples, &ccd)?;

    let tot = TypeOrigin {
        module_name: "module_name".to_string(),
        datatype_name: "datatype_name".to_string(),
        package: ObjectID::random(),
    };
    tracer.trace_value(&mut samples, &tot)?;

    // 2. Trace the main entry point(s) + every enum separately.
    tracer.trace_type::<Owner>(&samples)?;
    tracer.trace_type::<ExecutionStatus>(&samples)?;
    tracer.trace_type::<ExecutionFailureStatus>(&samples)?;
    tracer.trace_type::<CallArg>(&samples)?;
    tracer.trace_type::<ObjectArg>(&samples)?;
    tracer.trace_type::<Data>(&samples)?;
    tracer.trace_type::<TypeTag>(&samples)?;
    tracer.trace_type::<TypedStoreError>(&samples)?;
    tracer.trace_type::<ObjectInfoRequestKind>(&samples)?;
    tracer.trace_type::<TransactionKind>(&samples)?;
    tracer.trace_type::<MoveObjectType>(&samples)?;
    tracer.trace_type::<MoveObjectType_>(&samples)?;
    tracer.trace_type::<base_types::SuiAddress>(&samples)?;
    tracer.trace_type::<DeleteKind>(&samples)?;
    tracer.trace_type::<Argument>(&samples)?;
    tracer.trace_type::<Command>(&samples)?;
    tracer.trace_type::<CommandArgumentError>(&samples)?;
    tracer.trace_type::<TypeArgumentError>(&samples)?;
    tracer.trace_type::<PackageUpgradeError>(&samples)?;
    tracer.trace_type::<TransactionExpiration>(&samples)?;
    tracer.trace_type::<EndOfEpochTransactionKind>(&samples)?;

    tracer.trace_type::<IDOperation>(&samples)?;
    tracer.trace_type::<ObjectIn>(&samples)?;
    tracer.trace_type::<ObjectOut>(&samples)?;
    tracer.trace_type::<UnchangedSharedKind>(&samples)?;
    tracer.trace_type::<TransactionEffects>(&samples)?;

    // uncomment once GenericSignature is added
    tracer.trace_type::<FullCheckpointContents>(&samples)?;
    tracer.trace_type::<CheckpointContents>(&samples)?;
    tracer.trace_type::<CheckpointSummary>(&samples)?;

    tracer.registry()
}

#[derive(Debug, Parser, Clone, Copy, ValueEnum)]
enum Action {
    Print,
    Test,
    Record,
}

#[derive(Debug, Parser)]
#[clap(
    name = "Sui format generator",
    about = "Trace serde (de)serialization to generate format descriptions for Sui types"
)]
struct Options {
    #[clap(value_enum, default_value = "Print", ignore_case = true)]
    action: Action,
}

const FILE_PATH: &str = "sui-core/tests/staged/sui.yaml";

fn main() {
    let options = Options::parse();
    let registry = get_registry().unwrap();
    match options.action {
        Action::Print => {
            let content = serde_yaml::to_string(&registry).unwrap();
            println!("{content}");
        }
        Action::Record => {
            let content = serde_yaml::to_string(&registry).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{}", content).unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content: String = serde_yaml::to_string(&registry).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}
