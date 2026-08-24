// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Demonstration of the `move-execution-api` architecture: one generic driver, written purely
//! against the harness traits, runs unchanged over two Move subsystem versions --
//!
//! - **v4** (`move-execution/v4`): the frozen subsystem, whose VM still has `Type::TyParam` and
//!   returns declared (unsubstituted) function signatures;
//! - **tip** (serving as version 5): the in-development subsystem, whose `Type` has no
//!   type-parameter variant and substitutes signatures internally.
//!
//! [`demo_for_version`] is the mock of the dispatch that, in the real design, lives in
//! `sui-execution/latest`'s executor constructor and matches on
//! `protocol_config.move_execution_version()`. The tests assert both subsystems produce
//! identical results through the shared vocabulary (type tags and BCS bytes), even though their
//! type representations differ.

use std::collections::BTreeMap;

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_compiler::{Compiler, shared::NumericalAddress};
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    language_storage::{ModuleId, TypeTag},
};
use move_execution_api::{Linkage, MoveRuntimeHarness, MoveVerifierHarness};
use move_execution_tip::TipSubsystem;
use move_execution_v4::V4Subsystem;
use move_vm_config::verifier::VerifierConfig;

/// The demo package's address.
const ADDR: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 0x42;
    AccountAddress::new(address)
};

const DEMO_MODULE: &str = r#"
module 0x42::demo {
    public struct Box<T> has copy, drop { value: T }

    public fun id<T>(x: T): T { x }

    public fun make_box<T>(x: T): Box<T> { Box { value: x } }

    public fun add(a: u64, b: u64): u64 { a + b }
}
"#;

/// Everything the demo observes from a subsystem, rendered in the shared vocabulary
/// (`TypeTag` strings and BCS bytes) so reports from different subsystems are comparable.
#[derive(Debug, PartialEq, Eq)]
pub struct DemoReport {
    /// `id<u64>`'s substituted signature, as type tags: parameters then returns.
    pub id_signature: (Vec<String>, Vec<String>),
    /// `make_box<u64>`'s substituted return type, as a type tag.
    pub box_return: String,
    /// BCS result of executing `id<u64>(42)`.
    pub id_result: Vec<u8>,
    /// BCS result of executing `add(1, 2)`.
    pub add_result: Vec<u8>,
}

fn compile_demo_modules() -> Result<Vec<CompiledModule>> {
    let dir = tempfile::tempdir()?;
    let file_path = dir.path().join("demo.move");
    std::fs::write(&file_path, DEMO_MODULE)?;
    let (_, units) = Compiler::from_files(
        None,
        vec![file_path.to_str().unwrap().to_string()],
        vec![],
        [("std", NumericalAddress::parse_str("0x1").unwrap())]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    )
    .build_and_report()?;
    dir.close()?;
    Ok(units
        .into_iter()
        .map(|unit| unit.named_module.module)
        .collect())
}

/// The generic driver: everything below is written against the harness traits only, with no
/// knowledge of which subsystem is running -- this is the position `sui-adapter`,
/// `sui-move-natives`, and `sui-verifier` occupy in the full design.
pub fn demo<S>() -> Result<DemoReport>
where
    S: MoveRuntimeHarness + MoveVerifierHarness,
{
    let modules = compile_demo_modules()?;

    // Verification facet: base-verify every module through the subsystem's verifier.
    for module in &modules {
        S::verify_module(module, &VerifierConfig::default(), &mut DummyMeter)
            .map_err(|e| anyhow::anyhow!("verification failed: {e:?}"))?;
    }

    // Runtime facet: publish, link, resolve types, and execute.
    let mut runtime = S::new_runtime();
    S::publish_package(&mut runtime, ADDR, modules)
        .map_err(|e| anyhow::anyhow!("publish failed: {e:?}"))?;

    let linkage = Linkage {
        linkage_table: BTreeMap::from([(ADDR, ADDR)]),
    };
    let mut vm = S::make_vm(&runtime, &linkage).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let module_id = ModuleId::new(ADDR, ident_str!("demo").to_owned());
    let u64_type = S::load_type(&vm, &TypeTag::U64).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    // The harness contract: signatures come back substituted, whichever subsystem runs. (The v4
    // glue adapts its VM's declared signatures; the tip VM substitutes natively.)
    let tags = |vm: &S::Vm<'_>, tys: &[S::Type]| -> Result<Vec<String>> {
        tys.iter()
            .map(|ty| {
                Ok(S::type_tag(vm, ty)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?
                    .to_string())
            })
            .collect()
    };

    let id_sig = S::function_signature(
        &vm,
        &module_id,
        ident_str!("id"),
        std::slice::from_ref(&u64_type),
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let id_signature = (tags(&vm, &id_sig.parameters)?, tags(&vm, &id_sig.return_)?);

    let box_sig = S::function_signature(
        &vm,
        &module_id,
        ident_str!("make_box"),
        std::slice::from_ref(&u64_type),
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let box_return = tags(&vm, &box_sig.return_)?.pop().expect("one return type");

    let run = |vm: &mut S::Vm<'_>,
               name,
               ty_args: Vec<S::Type>,
               args: Vec<Vec<u8>>,
               ret_ty|
     -> Result<Vec<u8>> {
        let mut results = S::execute_function(vm, &module_id, name, ty_args, args)
            .map_err(|e| anyhow::anyhow!("execution failed: {e:?}"))?;
        let result = results.pop().expect("one return value");
        S::serialize_value(vm, &result, ret_ty).map_err(|e| anyhow::anyhow!("{e:?}"))
    };

    let id_result = run(
        &mut vm,
        ident_str!("id"),
        vec![u64_type.clone()],
        vec![bcs::to_bytes(&42u64)?],
        &TypeTag::U64,
    )?;
    let add_result = run(
        &mut vm,
        ident_str!("add"),
        vec![],
        vec![bcs::to_bytes(&1u64)?, bcs::to_bytes(&2u64)?],
        &TypeTag::U64,
    )?;

    Ok(DemoReport {
        id_signature,
        box_return,
        id_result,
        add_result,
    })
}

/// Mock of the per-epoch dispatch: in the full design this match lives in
/// `sui-execution/latest`'s executor constructor, keyed by
/// `protocol_config.move_execution_version()`.
pub fn demo_for_version(move_execution_version: u64) -> Result<DemoReport> {
    match move_execution_version {
        4 => demo::<V4Subsystem>(),
        5 => demo::<TipSubsystem>(),
        v => panic!("Unsupported Move execution version {v}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payoff line: the same generic driver, dispatched by version number, produces
    /// identical observable results on the frozen and tip subsystems.
    #[test]
    fn subsystems_agree() {
        let v4 = demo_for_version(4).expect("v4 subsystem runs");
        let v5 = demo_for_version(5).expect("tip subsystem runs");
        assert_eq!(v4, v5);
    }

    /// Both subsystems honor the post-substitution signature contract, including substitution
    /// into a generic datatype instantiation.
    #[test]
    fn signatures_are_substituted() {
        for version in [4, 5] {
            let report = demo_for_version(version).unwrap();
            assert_eq!(
                report.id_signature,
                (vec!["u64".to_string()], vec!["u64".to_string()]),
                "subsystem {version}"
            );
            assert_eq!(
                report.box_return, "0x42::demo::Box<u64>",
                "subsystem {version}"
            );
        }
    }

    /// Execution agrees in the shared BCS vocabulary.
    #[test]
    fn execution_results() {
        for version in [4, 5] {
            let report = demo_for_version(version).unwrap();
            assert_eq!(report.id_result, bcs::to_bytes(&42u64).unwrap());
            assert_eq!(report.add_result, bcs::to_bytes(&3u64).unwrap());
        }
    }
}
