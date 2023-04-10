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
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use std::{fs::File, io::Write};
use sui_types::{
    base_types::MoveObjectType_,
    crypto::Signer,
    messages_checkpoint::{CheckpointContentsDigest, CheckpointDigest, CheckpointSummary},
};
use sui_types::{
    base_types::{
        self, MoveObjectType, ObjectDigest, ObjectID, TransactionDigest, TransactionEffectsDigest,
    },
    crypto::{
        get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes,
        AuthoritySignature, KeypairTraits, Signature,
    },
    messages::{
        Argument, CallArg, Command, CommandArgumentError, ExecutionFailureStatus, ExecutionStatus,
        ObjectArg, ObjectInfoRequestKind, PackageUpgradeError, TransactionKind, TypeArgumentError,
    },
    object::{Data, Owner},
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

    // uncomment once GenericSignature is added
    // tracer.trace_type::<FullCheckpointContents>(&samples)?;
    // tracer.trace_type::<CheckpointContents>(&samples)?;
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
