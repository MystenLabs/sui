// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;
use move_core_types::{
    language_storage::TypeTag,
    value::{MoveStructLayout, MoveTypeLayout},
    vm_status::AbortLocation,
};
use pretty_assertions::assert_str_eq;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use std::{fs::File, io::Write};
use sui_types::execution_status::{
    CommandArgumentError, ExecutionFailureStatus, ExecutionStatus, PackageUpgradeError,
    TypeArgumentError,
};
use sui_types::{
    base_types::MoveObjectType_,
    crypto::Signer,
    messages::TransactionExpiration,
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSummary,
        FullCheckpointContents,
    },
};
use sui_types::{
    base_types::{
        self, MoveObjectType, ObjectDigest, ObjectID, TransactionDigest, TransactionEffectsDigest,
    },
    crypto::{
        get_key_pair, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
        AuthorityPublicKeyBytes, AuthoritySignature, KeypairTraits, Signature, SuiKeyPair,
    },
    messages::{Argument, CallArg, Command, ObjectArg, ObjectInfoRequestKind, TransactionKind},
    multisig::{MultiSig, MultiSigPublicKey},
    object::{Data, Owner},
    signature::GenericSignature,
    storage::DeleteKind,
};
use typed_store::rocks::TypedStoreError;

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

    let multisig_pk =
        MultiSigPublicKey::new(vec![kp1.public(), kp2.public()], vec![1, 1], 2).unwrap();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Message".as_bytes().to_vec(),
        },
    );

    let sig1 = Signature::new_secure(&msg, &kp1);
    let sig2 = Signature::new_secure(&msg, &kp2);
    let sig3 = Signature::new_secure(&msg, &kp3);

    let multi_sig = MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk).unwrap();
    tracer.trace_value(&mut samples, &multi_sig)?;

    let generic_sig_multi = GenericSignature::MultiSig(multi_sig);
    tracer.trace_value(&mut samples, &generic_sig_multi)?;

    let generic_sig_1 = GenericSignature::Signature(sig1);
    tracer.trace_value(&mut samples, &generic_sig_1)?;
    let generic_sig_2 = GenericSignature::Signature(sig2);
    tracer.trace_value(&mut samples, &generic_sig_2)?;
    let generic_sig_3 = GenericSignature::Signature(sig3);
    tracer.trace_value(&mut samples, &generic_sig_3)?;

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

    let ccd = CheckpointDigest::random();
    tracer.trace_value(&mut samples, &ccd)?;

    // 2. Trace the main entry point(s) + every enum separately.
    tracer.trace_type::<Owner>(&samples)?;
    tracer.trace_type::<ExecutionStatus>(&samples)?;
    tracer.trace_type::<ExecutionFailureStatus>(&samples)?;
    tracer.trace_type::<AbortLocation>(&samples)?;
    tracer.trace_type::<CallArg>(&samples)?;
    tracer.trace_type::<ObjectArg>(&samples)?;
    tracer.trace_type::<Data>(&samples)?;
    tracer.trace_type::<TypeTag>(&samples)?;
    tracer.trace_type::<TypedStoreError>(&samples)?;
    tracer.trace_type::<ObjectInfoRequestKind>(&samples)?;
    tracer.trace_type::<TransactionKind>(&samples)?;
    tracer.trace_type::<MoveStructLayout>(&samples)?;
    tracer.trace_type::<MoveTypeLayout>(&samples)?;
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

    // uncomment once GenericSignature is added
    tracer.trace_type::<FullCheckpointContents>(&samples)?;
    tracer.trace_type::<CheckpointContents>(&samples)?;
    tracer.trace_type::<CheckpointSummary>(&samples)?;

    tracer.registry()
}

#[derive(Debug, Parser, Clone, Copy, ArgEnum)]
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
    #[clap(arg_enum, default_value = "Print", ignore_case = true)]
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
            let content = serde_yaml::to_string(&registry).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}
