// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Demonstration of the `move-execution-api` architecture: one generic driver, written purely
//! against the harness traits, runs unchanged over three Move subsystems --
//!
//! - **`Version(4)`** (`move-execution/v4`): the frozen subsystem whose VM still has
//!   `Type::TyParam` and returns declared (unsubstituted) function signatures;
//! - **`Version(5)`** (`move-execution/v5`): the frozen subsystem with `TyParam` removed -- a
//!   pure contract refactor of v4, observationally identical (results *and* gas);
//! - **`Latest`** (the tip): diverged again -- `LdConst` now charges gas by the constant's
//!   abstract (in-memory) size instead of its serialized byte length, so it produces identical
//!   results but *different gas* from v5.
//!
//! Together the frozen pair and the tip demonstrate the two classes of subsystem change: a
//! behavior-preserving refactor (v4 ≡ v5) and a behavior-changing one (Latest), each shipped by
//! freezing the predecessor and pointing a protocol version at the successor.
//!
//! [`demo_for_version`] is the mock of the dispatch that, in the real design, lives in
//! `sui-execution/latest`'s executor constructor, keyed on
//! `protocol_config.move_execution_version()`.

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
pub use move_execution_api::MoveExecutionVersion;
use move_execution_api::{Linkage, MoveRuntimeHarness, MoveVerifierHarness};
use move_execution_latest::LatestSubsystem;
use move_execution_v4::V4Subsystem;
use move_execution_v5::V5Subsystem;
use move_vm_config::verifier::VerifierConfig;

/// The demo package's address.
const ADDR: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 0x42;
    AccountAddress::new(address)
};

/// How many (empty) inner vectors the `BIG` constant holds. A vector of empty vectors is the
/// starkest case of serialized size diverging from abstract size: every empty inner vector costs
/// a serialized byte but materializes to (almost) nothing in memory, so the divergence between
/// the two `LdConst` charging schemes scales with this count.
const BIG_CONSTANT_LEN: usize = 2048;

fn demo_module_source() -> String {
    let big = format!("vector[{}]", "vector[],".repeat(BIG_CONSTANT_LEN));
    format!(
        r#"
module 0x42::demo {{
    const GREETING: vector<u8> = b"hello from the demo package";

    const BIG: vector<vector<u8>> = {big};

    public struct Box<T> has copy, drop {{ value: T }}

    public fun id<T>(x: T): T {{ x }}

    public fun make_box<T>(x: T): Box<T> {{ Box {{ value: x }} }}

    public fun add(a: u64, b: u64): u64 {{ a + b }}

    public fun greeting(): vector<u8> {{ GREETING }}

    public fun load_big(): vector<vector<u8>> {{ BIG }}
}}
"#
    )
}

/// Everything the demo observes from a subsystem, rendered in the shared vocabulary
/// (`TypeTag` strings, BCS bytes, gas units) so reports from different subsystems are comparable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoReport {
    /// `id<u64>`'s substituted signature, as type tags: parameters then returns.
    pub id_signature: (Vec<String>, Vec<String>),
    /// `make_box<u64>`'s substituted return type, as a type tag.
    pub box_return: String,
    /// BCS result of executing `id<u64>(42)`.
    pub id_result: Vec<u8>,
    /// BCS result of executing `add(1, 2)`.
    pub add_result: Vec<u8>,
    /// BCS result of executing `greeting()`.
    pub greeting_result: Vec<u8>,
    /// BCS result of executing `load_big()` (an `LdConst` of the `BIG` constant).
    pub big_constant_result: Vec<u8>,
    /// Gas consumed by the metered `load_big()` execution. This is where the `Latest`
    /// subsystem's `LdConst` change is visible.
    pub big_constant_gas: u64,
}

impl DemoReport {
    /// The report with gas erased: what "observationally identical, gas aside" compares.
    pub fn results_only(&self) -> DemoReport {
        DemoReport {
            big_constant_gas: 0,
            ..self.clone()
        }
    }
}

fn compile_demo_modules() -> Result<Vec<CompiledModule>> {
    let dir = tempfile::tempdir()?;
    let file_path = dir.path().join("demo.move");
    std::fs::write(&file_path, demo_module_source())?;
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
    // glue adapts its VM's declared signatures; v5 and Latest substitute natively.)
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

    let vec_u8_tag = TypeTag::Vector(Box::new(TypeTag::U8));
    let greeting_result = run(
        &mut vm,
        ident_str!("greeting"),
        vec![],
        vec![],
        &vec_u8_tag.clone(),
    )?;

    // Metered execution of the constant-loading function: this is where a gas-behavior change
    // between subsystems becomes visible.
    let vec_vec_u8_tag = TypeTag::Vector(Box::new(vec_u8_tag));
    let (mut big_values, big_constant_gas) =
        S::execute_function_metered(&mut vm, &module_id, ident_str!("load_big"), vec![], vec![])
            .map_err(|e| anyhow::anyhow!("metered execution failed: {e:?}"))?;
    let big_value = big_values.pop().expect("one return value");
    let big_constant_result = S::serialize_value(&vm, &big_value, &vec_vec_u8_tag)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    Ok(DemoReport {
        id_signature,
        box_return,
        id_result,
        add_result,
        greeting_result,
        big_constant_result,
        big_constant_gas,
    })
}

/// Mock of the per-epoch dispatch: in the full design this match lives in
/// `sui-execution/latest`'s executor constructor, keyed by
/// `protocol_config.move_execution_version()`.
pub fn demo_for_version(move_execution_version: MoveExecutionVersion) -> Result<DemoReport> {
    match move_execution_version {
        MoveExecutionVersion::Version(4) => demo::<V4Subsystem>(),
        MoveExecutionVersion::Version(5) => demo::<V5Subsystem>(),
        MoveExecutionVersion::Latest => demo::<LatestSubsystem>(),
        // `Unspecified` predates the selector: those protocol versions never reach a dispatch
        // like this one -- their execution layer is hardwired to its subsystem.
        v => panic!("Unsupported Move execution version {v:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A behavior-preserving subsystem change: v5 (TyParam removed, substitution moved into the
    /// VM) is a pure contract refactor of v4, so the two frozen subsystems agree on everything
    /// the driver can observe -- results *and* gas.
    #[test]
    fn frozen_subsystems_agree() {
        let v4 = demo_for_version(MoveExecutionVersion::Version(4)).expect("v4 runs");
        let v5 = demo_for_version(MoveExecutionVersion::Version(5)).expect("v5 runs");
        assert_eq!(v4, v5);
    }

    /// A behavior-changing subsystem change: Latest charges `LdConst` by abstract size instead
    /// of serialized size. Results are identical to v5; gas is not. This is exactly the class of
    /// change that must ride a freeze + protocol flip rather than land in place.
    #[test]
    fn latest_changes_gas_only() {
        let v5 = demo_for_version(MoveExecutionVersion::Version(5)).expect("v5 runs");
        let latest = demo_for_version(MoveExecutionVersion::Latest).expect("latest runs");
        assert_eq!(v5.results_only(), latest.results_only());
        assert_ne!(
            v5.big_constant_gas, latest.big_constant_gas,
            "LdConst gas should differ between serialized-size and abstract-size charging"
        );
    }

    /// All subsystems honor the post-substitution signature contract, including substitution
    /// into a generic datatype instantiation.
    #[test]
    fn signatures_are_substituted() {
        for version in [
            MoveExecutionVersion::Version(4),
            MoveExecutionVersion::Version(5),
            MoveExecutionVersion::Latest,
        ] {
            let report = demo_for_version(version).unwrap();
            assert_eq!(
                report.id_signature,
                (vec!["u64".to_string()], vec!["u64".to_string()]),
                "subsystem {version:?}"
            );
            assert_eq!(
                report.box_return, "0x42::demo::Box<u64>",
                "subsystem {version:?}"
            );
        }
    }

    /// Execution agrees in the shared BCS vocabulary across all subsystems.
    #[test]
    fn execution_results() {
        for version in [
            MoveExecutionVersion::Version(4),
            MoveExecutionVersion::Version(5),
            MoveExecutionVersion::Latest,
        ] {
            let report = demo_for_version(version).unwrap();
            assert_eq!(report.id_result, bcs::to_bytes(&42u64).unwrap());
            assert_eq!(report.add_result, bcs::to_bytes(&3u64).unwrap());
            assert_eq!(
                report.greeting_result,
                bcs::to_bytes(&b"hello from the demo package".to_vec()).unwrap()
            );
            assert_eq!(
                report.big_constant_result,
                bcs::to_bytes(&vec![Vec::<u8>::new(); BIG_CONSTANT_LEN]).unwrap()
            );
        }
    }
}
