// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module provides a checker for verifying that data definitions in a module are not
//! recursive. Since the module dependency graph is acylic by construction, applying this checker to
//! each module in isolation guarantees that there is no structural recursion globally.
use move_binary_format::{
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        CompiledModule, DatatypeHandleIndex, EnumDefinitionIndex, SignatureToken,
        StructDefinitionIndex, TableIndex,
    },
    internals::ModuleIndex,
    IndexKind,
};
use move_core_types::vm_status::StatusCode;
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use std::collections::{BTreeMap, BTreeSet};

pub struct RecursiveDataDefChecker<'a> {
    module: &'a CompiledModule,
}

impl<'a> RecursiveDataDefChecker<'a> {
    pub fn verify_module(module: &'a CompiledModule) -> VMResult<()> {
        Self::verify_module_impl(module).map_err(|e| e.finish(Location::Module(module.self_id())))
    }

    fn verify_module_impl(module: &'a CompiledModule) -> PartialVMResult<()> {
        let checker = Self { module };
        let graph = DataDefGraphBuilder::new(checker.module)?.build()?;

        // toposort is iterative while petgraph::algo::is_cyclic_directed is recursive. Prefer
        // the iterative solution here as this code may be dealing with untrusted data.
        match toposort(&graph, None) {
            Ok(_) => Ok(()),
            Err(cycle) => match cycle.node_id() {
                DataIndex::Struct(idx) => Err(verification_error(
                    StatusCode::RECURSIVE_DATATYPE_DEFINITION,
                    IndexKind::StructDefinition,
                    idx,
                )),
                DataIndex::Enum(idx) => Err(verification_error(
                    StatusCode::RECURSIVE_DATATYPE_DEFINITION,
                    IndexKind::EnumDefinition,
                    idx,
                )),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum DataIndex {
    Struct(TableIndex),
    Enum(TableIndex),
}

/// Given a module, build a graph of data definitions. This is useful when figuring out whether
/// the data definitions in the module form a cycle.
struct DataDefGraphBuilder<'a> {
    module: &'a CompiledModule,
    /// Used to follow field definitions' signatures' data handles to their data definitions.
    handle_to_def: BTreeMap<DatatypeHandleIndex, DataIndex>,
}

impl<'a> DataDefGraphBuilder<'a> {
    fn new(module: &'a CompiledModule) -> PartialVMResult<Self> {
        let mut handle_to_def = BTreeMap::new();
        // the mapping from data definitions to data handles is already checked to be 1-1 by
        // DuplicationChecker
        for (idx, struct_def) in module.struct_defs().iter().enumerate() {
            let sh_idx = struct_def.struct_handle;
            if let Some(other) = handle_to_def.insert(sh_idx, DataIndex::Struct(idx as TableIndex))
            {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "Duplicate struct handle index {} for struct definitions {:?} and {}",
                            sh_idx, other, idx
                        )),
                );
            }
        }

        for (idx, enum_def) in module.enum_defs().iter().enumerate() {
            let sh_idx = enum_def.enum_handle;
            if let Some(other) = handle_to_def.insert(sh_idx, DataIndex::Enum(idx as TableIndex)) {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "Duplicate enum handle index {} for enum definitions {:?} and {}",
                            sh_idx, other, idx
                        )),
                );
            }
        }

        Ok(Self {
            module,
            handle_to_def,
        })
    }

    fn build(self) -> PartialVMResult<DiGraphMap<DataIndex, ()>> {
        let mut neighbors = BTreeMap::new();
        for idx in 0..self.module.struct_defs().len() {
            let sd_idx = StructDefinitionIndex::new(idx as TableIndex);
            self.add_struct_defs(&mut neighbors, sd_idx)?
        }

        for idx in 0..self.module.enum_defs().len() {
            let sd_idx = EnumDefinitionIndex::new(idx as TableIndex);
            self.add_enum_defs(&mut neighbors, sd_idx)?
        }

        let edges = neighbors
            .into_iter()
            .flat_map(|(parent, children)| children.into_iter().map(move |child| (parent, child)));
        Ok(DiGraphMap::from_edges(edges))
    }

    fn add_struct_defs(
        &self,
        neighbors: &mut BTreeMap<DataIndex, BTreeSet<DataIndex>>,
        idx: StructDefinitionIndex,
    ) -> PartialVMResult<()> {
        let struct_def = self.module.struct_def_at(idx);
        // The fields iterator is an option in the case of native structs. Flatten makes an empty
        // iterator for that case
        for field in struct_def.fields().into_iter().flatten() {
            self.add_signature_token(
                neighbors,
                DataIndex::Struct(idx.into_index() as TableIndex),
                &field.signature.0,
            )?
        }
        Ok(())
    }

    fn add_enum_defs(
        &self,
        neighbors: &mut BTreeMap<DataIndex, BTreeSet<DataIndex>>,
        idx: EnumDefinitionIndex,
    ) -> PartialVMResult<()> {
        let enum_def = self.module.enum_def_at(idx);
        for field in enum_def
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
        {
            self.add_signature_token(
                neighbors,
                DataIndex::Enum(idx.into_index() as TableIndex),
                &field.signature.0,
            )?
        }
        Ok(())
    }

    fn add_signature_token(
        &self,
        neighbors: &mut BTreeMap<DataIndex, BTreeSet<DataIndex>>,
        cur_idx: DataIndex,
        token: &SignatureToken,
    ) -> PartialVMResult<()> {
        use SignatureToken as T;
        match token {
            T::Bool
            | T::U8
            | T::U16
            | T::U32
            | T::U64
            | T::U128
            | T::U256
            | T::Address
            | T::Signer
            | T::TypeParameter(_) => (),
            T::Reference(_) | T::MutableReference(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("Reference field when checking recursive structs".to_owned()),
                )
            }
            T::Vector(inner) => self.add_signature_token(neighbors, cur_idx, inner)?,
            T::Datatype(sh_idx) => {
                if let Some(data_def_idx) = self.handle_to_def.get(sh_idx) {
                    neighbors.entry(cur_idx).or_default().insert(*data_def_idx);
                }
            }
            T::DatatypeInstantiation(inst) => {
                let (sh_idx, inners) = &**inst;
                if let Some(data_def_idx) = self.handle_to_def.get(sh_idx) {
                    neighbors.entry(cur_idx).or_default().insert(*data_def_idx);
                }
                for t in inners {
                    self.add_signature_token(neighbors, cur_idx, t)?
                }
            }
        };
        Ok(())
    }
}
