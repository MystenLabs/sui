// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use itertools::Itertools;
use move_async_vm::{
    actor_metadata,
    actor_metadata::ActorMetadata,
    async_vm::{AsyncResult, AsyncSession, AsyncVM, Message},
    natives::GasParameters as ActorGasParameters,
};
use move_binary_format::access::ModuleAccess;
use move_command_line_common::testing::EXP_EXT;
use move_compiler::{
    compiled_unit::CompiledUnit, diagnostics::report_diagnostics_to_buffer,
    shared::NumericalAddress, Compiler, Flags,
};
use move_core_types::{
    account_address::AccountAddress,
    effects::{ChangeSet, Op},
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use move_prover_test_utils::{baseline_test::verify_or_update_baseline, extract_test_directives};
use move_vm_test_utils::gas_schedule::GasStatus;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    str::FromStr,
};

const TEST_ADDR: &str = "0x3";
const SOURCE_DIRS: &[&str] = &[
    "./tests/sources",
    "../move-async-lib/sources",
    "../../../move-stdlib/sources",
];

struct Harness {
    module_cache: BTreeMap<Identifier, CompiledUnit>,
    vm: AsyncVM,
    actor_instances: Vec<(ModuleId, AccountAddress)>,
    baseline: RefCell<String>,
    resource_store: RefCell<BTreeMap<(AccountAddress, StructTag), Vec<u8>>>,
}

fn test_account() -> AccountAddress {
    AccountAddress::from_hex_literal(TEST_ADDR).expect("valid test address")
}

fn test_runner(path: &Path) -> datatest_stable::Result<()> {
    let target_module = path
        .with_extension("")
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let deps = extract_test_directives(path, "// dep:")?;
    let actors = extract_test_directives(path, "// actor: ")?;
    let instances = extract_test_directives(path, "// instance: ")?;
    let harness = Harness::new(
        std::iter::once(target_module.clone())
            .chain(deps.into_iter())
            .collect(),
        actors,
        instances,
    )?;
    harness.run(&target_module)?;
    let baseline_path = path.with_extension(EXP_EXT);
    verify_or_update_baseline(baseline_path.as_path(), harness.baseline.borrow().as_str())?;
    Ok(())
}

datatest_stable::harness!(test_runner, "tests/sources", r".*\.move");

// ========================================================================================
// Test execution

impl Harness {
    fn run(&self, _module: &str) -> anyhow::Result<()> {
        let mut gas = GasStatus::new_unmetered();
        let mut tick = 0;
        // Publish modules.
        let mut proxy = HarnessProxy { harness: self };
        let mut session = self.vm.new_session(test_account(), 0, &mut proxy);
        let mut done = BTreeSet::new();
        for id in self.module_cache.keys() {
            self.publish_module(&mut session, id, &mut gas, &mut done)?;
        }
        // Initialize actors
        let mut mailbox: VecDeque<Message> = Default::default();
        for (actor, addr) in self.actor_instances.clone() {
            self.log(format!(
                "actor 0x{} created from {}",
                addr.short_str_lossless(),
                actor.short_str_lossless()
            ));
            {
                let mut proxy = HarnessProxy { harness: self };
                let session = self.vm.new_session(addr, 0, &mut proxy);
                let result = session.new_actor(&actor, addr, &mut gas);
                self.handle_result(&mut mailbox, result);
            };

            // Put a start message for this actor into the mailbox.
            let entry_point_id = Identifier::from_str("start")?;
            let hash = actor_metadata::message_hash(&actor, &entry_point_id);
            mailbox.push_back((addr, hash, vec![]));
        }

        // Handle messages until the mailbox is empty.
        while let Some((actor, message_hash, args)) = mailbox.pop_front() {
            // Baseline logging
            if let Some((module_id, fun_id)) = self.vm.resolve_message_hash(message_hash).cloned() {
                self.log(format!(
                    "actor 0x{} handling {}::{} (hash=0x{:X})",
                    actor.short_str_lossless(),
                    module_id.short_str_lossless(),
                    fun_id,
                    message_hash
                ));
            } else {
                self.log(format!(
                    "actor 0x{} handling ???? (hash={})",
                    actor.short_str_lossless(),
                    message_hash
                ))
            }
            // Handling
            let mut proxy = HarnessProxy { harness: self };
            let session = self.vm.new_session(actor, tick, &mut proxy);
            tick += 1000_1000; // micros
            let result = session.handle_message(actor, message_hash, args, &mut gas);
            self.handle_result(&mut mailbox, result);
        }
        Ok(())
    }

    fn publish_module(
        &self,
        session: &mut AsyncSession<HarnessProxy>,
        id: &IdentStr,
        gas: &mut GasStatus,
        done: &mut BTreeSet<Identifier>,
    ) -> anyhow::Result<()> {
        if done.insert(id.to_owned()) {
            let cu = self.module_cache.get(id).unwrap();
            if let CompiledUnit::Module(m) = cu {
                for dep in &m.module.module_handles {
                    let dep_id = m.module.identifier_at(dep.name);
                    self.publish_module(session, dep_id, gas, done)?
                }
            }
            self.log(format!("publishing {}", id));
            session
                .get_move_session()
                .publish_module(cu.serialize(None), test_account(), gas)?
        }
        Ok(())
    }

    fn handle_result(&self, mailbox: &mut VecDeque<Message>, result: AsyncResult) {
        match result {
            Ok(success) => {
                self.log("  SUCCESS");
                for m in &success.messages {
                    self.log(format!(
                        "  sent 0x{} <- 0x{:X} argc={}",
                        m.0.short_str_lossless(),
                        m.1,
                        m.2.len()
                    ))
                }
                mailbox.extend(success.messages);
                self.commit_changeset(success.change_set)
            }
            Err(error) => self.log(format!("  FAIL  {:}", error)),
        }
    }

    fn commit_changeset(&self, changeset: ChangeSet) {
        for (addr, change) in changeset.into_inner() {
            for (struct_tag, op) in change.into_inner().1 {
                self.log(format!(
                    "  commit 0x{}::{}::{}[0x{}] := {:?}",
                    struct_tag.address.short_str_lossless(),
                    struct_tag.module,
                    struct_tag.module,
                    addr.short_str_lossless(),
                    op.as_ref().map(|b| format!("{:02X?}", b))
                ));
                match op {
                    Op::New(v) => {
                        assert!(self
                            .resource_store
                            .borrow_mut()
                            .insert((addr, struct_tag), v)
                            .is_none());
                    }
                    Op::Modify(v) => {
                        self.resource_store
                            .borrow_mut()
                            .insert((addr, struct_tag), v)
                            .unwrap();
                    }
                    Op::Delete => {
                        self.resource_store
                            .borrow_mut()
                            .remove(&(addr, struct_tag))
                            .unwrap();
                    }
                }
            }
        }
    }
}

// ========================================================================================
// Harness creation

impl Harness {
    fn new(
        modules: Vec<String>,
        actors: Vec<String>,
        instances: Vec<String>,
    ) -> anyhow::Result<Self> {
        // Create address map. We are mapping all aliases to TEST_ADDR for simplicity.
        let test_addr = NumericalAddress::parse_str(TEST_ADDR).unwrap();
        let address_map: BTreeMap<String, NumericalAddress> = vec![
            ("std".to_string(), test_addr),
            ("Async".to_string(), test_addr),
            ("Test".to_string(), test_addr),
        ]
        .into_iter()
        .collect();
        // Collect metadata and compile modules.
        let actor_metadata = Self::collect_metadata(actors)?;
        let actor_instances = Self::collect_instances(instances)?;
        let module_files = Self::collect_modules(modules)?;
        let module_cache = Self::compile(&address_map, &module_files)?;
        let harness = Harness {
            baseline: Default::default(),
            module_cache,
            resource_store: Default::default(),
            vm: AsyncVM::new(
                test_account(),
                move_stdlib::natives::all_natives(
                    test_account(),
                    // We may want to switch to a different gas schedule in the future, but for now,
                    // the all-zero one should be enough.
                    move_stdlib::natives::GasParameters::zeros(),
                ),
                actor_metadata,
                ActorGasParameters::zeros(),
            )?,
            actor_instances,
        };
        Ok(harness)
    }

    fn collect_metadata(actors: Vec<String>) -> anyhow::Result<Vec<ActorMetadata>> {
        let mut actor_metadata = vec![];
        for actor in actors {
            // format: 0x3 Module State init message..
            let parts = actor.split_ascii_whitespace().collect_vec();
            if parts.len() < 4 {
                bail!("malformed actor decl `{}`", actor)
            }
            let address = AccountAddress::from_hex_literal(parts[0])?;
            let module = Identifier::from_str(parts[1])?;
            let struct_ = Identifier::from_str(parts[2])?;
            let initializer = Identifier::from_str(parts[3])?;
            let state_tag = StructTag {
                address,
                module: module.clone(),
                name: struct_,
                type_params: vec![],
            };
            let mut messages = vec![];
            for message in &parts[4..] {
                messages.push(Identifier::from_str(message)?)
            }
            actor_metadata.push(ActorMetadata {
                module_id: ModuleId::new(address, module),
                state_tag,
                initializer,
                messages,
            })
        }
        Ok(actor_metadata)
    }

    fn collect_instances(
        instances: Vec<String>,
    ) -> anyhow::Result<Vec<(ModuleId, AccountAddress)>> {
        let mut result = vec![];
        for inst in instances {
            // format: 0x3 Module 0x23
            // where the last address is where the instance is to create
            let parts = inst.split_ascii_whitespace().collect_vec();
            if parts.len() != 3 {
                bail!("malformed instance decl `{}`", inst)
            }
            let address = AccountAddress::from_hex_literal(parts[0])?;
            let module = Identifier::from_str(parts[1])?;
            let inst_address = AccountAddress::from_hex_literal(parts[2])?;
            result.push((ModuleId::new(address, module), inst_address))
        }
        Ok(result)
    }

    fn collect_modules(deps: Vec<String>) -> anyhow::Result<BTreeMap<Identifier, String>> {
        let mut module_files = BTreeMap::new();
        for dep in &deps {
            let mut found = false;
            for dir in SOURCE_DIRS {
                let mut path = PathBuf::from(dir);
                path.push(format!("{}.move", dep));
                if path.exists() {
                    module_files.insert(
                        Identifier::from_str(dep).expect("valid identifier"),
                        path.to_string_lossy().to_string(),
                    );
                    found = true;
                    break;
                }
            }
            if !found {
                bail!("dependency {} not found", dep)
            }
        }
        Ok(module_files)
    }

    fn compile(
        address_map: &BTreeMap<String, NumericalAddress>,
        module_files: &BTreeMap<Identifier, String>,
    ) -> anyhow::Result<BTreeMap<Identifier, CompiledUnit>> {
        let mut module_cache = BTreeMap::new();
        for (id, path) in module_files {
            let targets = vec![path.to_owned()];
            let deps = module_files
                .values()
                .filter(|p| *p != path)
                .cloned()
                .collect();
            let compiler = Compiler::from_files(targets, deps, address_map.clone())
                .set_flags(Flags::empty().set_flavor("async"));
            let (sources, inner) = compiler.build()?;
            match inner {
                Err(diags) => bail!(
                    "Compilation failure {{\n{}\n}}",
                    String::from_utf8_lossy(
                        report_diagnostics_to_buffer(&sources, diags).as_slice()
                    )
                ),
                Ok((mut units, _)) => {
                    module_cache.insert(id.to_owned(), units.remove(0).into_compiled_unit());
                }
            }
        }
        Ok(module_cache)
    }
}

// ========================================================================================
// Move Resolver

/// A facade for the harness which can appear as mutable, even though the harness
/// is not. Keeping the harness immutable and using RefCell for the few mutation points
/// simplifies things in this test.
struct HarnessProxy<'a> {
    harness: &'a Harness,
}

impl<'a> LinkageResolver for HarnessProxy<'a> {
    type Error = ();
}

impl<'a> ModuleResolver for HarnessProxy<'a> {
    type Error = ();

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .harness
            .module_cache
            .get(id.name())
            .map(|c| c.serialize(None)))
    }
}

impl<'a> ResourceResolver for HarnessProxy<'a> {
    type Error = ();

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let res = self
            .harness
            .resource_store
            .borrow()
            .get(&(*address, typ.clone()))
            .cloned();
        Ok(res)
    }
}

// ========================================================================================
// Baseline writer

impl Harness {
    fn log(&self, s: impl ToString) {
        let s = s.to_string();
        self.baseline.borrow_mut().push_str(&(s + "\n"))
    }
}
