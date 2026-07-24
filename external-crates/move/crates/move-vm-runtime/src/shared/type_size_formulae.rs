// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Type size formulae.
//!
//! The VM bounds four quantities of a type, each against its own limit:
//!
//! - `type_size`: the syntactic node count of the type;
//! - `type_depth`: the syntactic depth of the type;
//! - `value_depth`: the depth of a *value* of the type, through datatype fields;
//! - `layout_size`: the node count of the type's generated layout, through datatype fields.
//!
//! Rather than realize a type and measure it, we predict each quantity with a closed-form
//! formula and check the prediction, so rejection is arithmetic and no oversized type, value,
//! or layout is ever built.
//!
//! Each quantity lives in one of two algebras — additive ([`LinearForm`], for `type_size` and
//! `layout_size`) and max-plus ([`MaxPlusForm`], for `type_depth` and `value_depth`) — both flat
//! and closed under substitution. The pipeline is a two-stage partial evaluator over three
//! operations: **resolve**, **substitute**, and **solve**.
//!
//! 1. **JIT** builds an [`ArenaTypeSizeFormula`] per datatype: the four forms over the
//!    datatype's own type parameters. `type_size`/`type_depth` are closed; `value_depth`/
//!    `layout_size` additionally carry the datatype's field *applications*, whose resolution
//!    needs a linkage.
//! 2. **Resolve** ([`ArenaTypeSizeFormula::resolve`]) closes the applications against a linkage
//!    (the vtable) into a flat [`PartialTypeSizeFormula`], using the vtable for recursive key
//!    resolution. Resolving a non-datatype ([`VMDispatchTables::arena_type_size_formula`])
//!    performs a recursive walk, and **substitute** ([`PartialTypeSizeFormula::substitute`])
//!    composes flat formulae into each other to model datatype instantiation.
//! 3. **Solve** ([`PartialTypeSizeFormula::solve`]) evaluates a flat form against concrete
//!    argument [`TypeSize`]s, yielding a concrete [`TypeSize`].
//!
//! All four forms are used for various safety checks across VM execution. All arithmetic
//! saturates: every quantity exists only to be compared against a limit, and a saturated value
//! exceeds any limit — the correct verdict.

use crate::{
    cache::arena::{ArenaBuilder, ArenaVec},
    execution::dispatch_tables::{VMDispatchTables, VirtualTableKey},
    jit::execution::ast::{ArenaType, Datatype},
    shared::{
        constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
        vm_pointer::VMPointer,
    },
};
use move_binary_format::{
    checked_as,
    errors::{PartialVMError, PartialVMResult},
    partial_vm_error,
};
use std::collections::HashSet;

/// Index of a type parameter.
pub(crate) type TyParamIndex = u16;

/// Tracks the datatypes currently being resolved, to guard against cyclic datatype graphs.
/// A `HashSet` is fine (and never a determinism hazard): it is only ever probed for membership
/// (`insert`/`remove`/`contains`) and never iterated, so its ordering never affects a result.
pub(crate) type Visiting = HashSet<VirtualTableKey>;

// -------------------------------------------------------------------------------------------------
// TypeSize
// -------------------------------------------------------------------------------------------------

/// The four size quantities of a concrete type. A call frame's type arguments carry these, so
/// every later limit check against them is arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSize {
    pub type_size: u64,
    pub type_depth: u64,
    pub value_depth: u64,
    pub layout_size: u64,
}

impl TypeSize {
    /// The sizes of a non-composite ("primitive") type: one node, one level, in every measure.
    pub(crate) const PRIMITIVE: TypeSize = TypeSize {
        type_size: 1,
        type_depth: 1,
        value_depth: 1,
        layout_size: 1,
    };

    /// The sizes of `vector<inner>` / `&inner` / `&mut inner`: one node and one level on top of
    /// the element in every measure.
    pub(crate) fn wrap(inner: TypeSize) -> TypeSize {
        TypeSize {
            type_size: inner.type_size.saturating_add(1),
            type_depth: inner.type_depth.saturating_add(1),
            value_depth: inner.value_depth.saturating_add(1),
            layout_size: inner.layout_size.saturating_add(1),
        }
    }
}

/// Check a solved `(type_size, type_depth)` against the type-traversal limits: depth first,
/// then size.
pub(crate) fn check_syntactic_limits(type_size: u64, type_depth: u64) -> PartialVMResult<()> {
    if type_depth > TYPE_DEPTH_MAX {
        return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
    }
    if type_size > MAX_TYPE_INSTANTIATION_NODES {
        return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
    }
    Ok(())
}

fn out_of_bounds_parameter(param: TyParamIndex, len: usize) -> PartialVMError {
    partial_vm_error!(
        UNKNOWN_INVARIANT_VIOLATION_ERROR,
        "type parameter {param} out of bounds -- len {len}"
    )
}

// -------------------------------------------------------------------------------------------------
// Flat forms
// -------------------------------------------------------------------------------------------------

/// One term of a [`LinearForm`]: `coefficient · x_param`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinearTerm {
    pub(crate) param: TyParamIndex,
    pub(crate) coefficient: u64,
}

/// One term of a [`MaxPlusForm`]: `offset + x_param`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MaxPlusTerm {
    pub(crate) param: TyParamIndex,
    pub(crate) offset: u64,
}

/// A flat additive form: `constant + Σ terms[i].coefficient · x_{terms[i].param}`. Terms are
/// sparse and merged by summing coefficients on the same parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinearForm {
    pub(crate) constant: u64,
    pub(crate) terms: Vec<LinearTerm>,
}

/// A flat max-plus form: `max(constant, maxᵢ(terms[i].offset + x_{terms[i].param}))`. Terms are
/// sparse and merged by taking the maximum offset on the same parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaxPlusForm {
    pub(crate) constant: u64,
    pub(crate) terms: Vec<MaxPlusTerm>,
}

impl LinearForm {
    pub(crate) fn constant(constant: u64) -> Self {
        Self {
            constant,
            terms: vec![],
        }
    }

    /// The form of a bare parameter: `x_param`.
    pub(crate) fn parameter(param: TyParamIndex) -> Self {
        Self {
            constant: 0,
            terms: vec![LinearTerm {
                param,
                coefficient: 1,
            }],
        }
    }

    /// Add `multiplicity` copies of `other` into this form: `self += multiplicity * other`,
    /// summing coefficients on shared parameters.
    ///
    /// Example: `self = 2 + 3·x0`, `other = 1 + x1`, `multiplicity = 2` gives
    /// `2 + 2·1 + 3·x0 + 2·x1 = 4 + 3·x0 + 2·x1`.
    pub(crate) fn absorb(&mut self, multiplicity: u64, other: &LinearForm) {
        self.constant = self
            .constant
            .saturating_add(multiplicity.saturating_mul(other.constant));
        for term in &other.terms {
            let scaled = multiplicity.saturating_mul(term.coefficient);
            match self.terms.iter_mut().find(|t| t.param == term.param) {
                Some(existing) => {
                    existing.coefficient = existing.coefficient.saturating_add(scaled)
                }
                None => self.terms.push(LinearTerm {
                    param: term.param,
                    coefficient: scaled,
                }),
            }
        }
    }

    /// Substitute a form for each parameter (positionally): replace every `xi` with `args[i]`,
    /// yielding another flat linear form (the algebra is closed under substitution).
    ///
    /// Example: `self = 1 + 2·x0`, `args = [3 + x1]` gives `1 + 2·(3 + x1) = 7 + 2·x1`.
    pub(crate) fn substitute(&self, args: &[LinearForm]) -> PartialVMResult<LinearForm> {
        let mut result = LinearForm::constant(self.constant);
        for term in &self.terms {
            let arg = args
                .get(term.param as usize)
                .ok_or_else(|| out_of_bounds_parameter(term.param, args.len()))?;
            result.absorb(term.coefficient, arg);
        }
        result.canonicalize();
        Ok(result)
    }

    /// Evaluate with a concrete value per parameter: substitute each `xi` with `args[i]` and add
    /// it all up.
    ///
    /// Example: `self = 1 + 2·x0 + x1`, `args = [4, 5]` gives `1 + 2·4 + 5 = 14`.
    pub(crate) fn solve(&self, args: &[u64]) -> PartialVMResult<u64> {
        let mut acc = self.constant;
        for term in &self.terms {
            let value = args
                .get(term.param as usize)
                .ok_or_else(|| out_of_bounds_parameter(term.param, args.len()))?;
            acc = acc.saturating_add(term.coefficient.saturating_mul(*value));
        }
        Ok(acc)
    }

    fn canonicalize(&mut self) {
        self.terms.sort_unstable_by_key(|t| t.param);
    }
}

impl MaxPlusForm {
    pub(crate) fn constant(constant: u64) -> Self {
        Self {
            constant,
            terms: vec![],
        }
    }

    /// The form of a bare parameter: `x_param`.
    pub(crate) fn parameter(param: TyParamIndex) -> Self {
        Self {
            constant: 0,
            terms: vec![MaxPlusTerm { param, offset: 0 }],
        }
    }

    /// Fold `other`, shifted up by `offset`, into this form under max-plus: `self = max(self,
    /// offset + other)`, keeping the larger offset on shared parameters. (This is the max-plus
    /// analogue of [`LinearForm::absorb`]: `max` replaces `+`, `+` replaces `*`.)
    ///
    /// Example: `self = max(2, 1+x0)`, `other = max(1, x1)`, `offset = 3` gives
    /// `max(2, 3+1, 1+x0, 3+x1) = max(4, 1+x0, 3+x1)`.
    pub(crate) fn absorb(&mut self, offset: u64, other: &MaxPlusForm) {
        self.constant = self.constant.max(offset.saturating_add(other.constant));
        for term in &other.terms {
            let shifted = offset.saturating_add(term.offset);
            match self.terms.iter_mut().find(|t| t.param == term.param) {
                Some(existing) => existing.offset = existing.offset.max(shifted),
                None => self.terms.push(MaxPlusTerm {
                    param: term.param,
                    offset: shifted,
                }),
            }
        }
    }

    /// Substitute a form for each parameter (positionally): replace every `xi` with `args[i]`,
    /// yielding another flat max-plus form (the algebra is closed under substitution).
    ///
    /// Example: `self = max(1, 2+x0)`, `args = [max(0, x1)]` gives
    /// `max(1, 2 + max(0, x1)) = max(2, 2+x1)`.
    pub(crate) fn substitute(&self, args: &[MaxPlusForm]) -> PartialVMResult<MaxPlusForm> {
        let mut result = MaxPlusForm::constant(self.constant);
        for term in &self.terms {
            let arg = args
                .get(term.param as usize)
                .ok_or_else(|| out_of_bounds_parameter(term.param, args.len()))?;
            result.absorb(term.offset, arg);
        }
        result.canonicalize();
        Ok(result)
    }

    /// Evaluate with a concrete value per parameter: substitute each `xi` with `args[i]` and take
    /// the max.
    ///
    /// Example: `self = max(1, 2+x0, x1)`, `args = [3, 5]` gives `max(1, 2+3, 5) = 5`.
    pub(crate) fn solve(&self, args: &[u64]) -> PartialVMResult<u64> {
        let mut acc = self.constant;
        for term in &self.terms {
            let value = args
                .get(term.param as usize)
                .ok_or_else(|| out_of_bounds_parameter(term.param, args.len()))?;
            acc = acc.max(term.offset.saturating_add(*value));
        }
        Ok(acc)
    }

    fn canonicalize(&mut self) {
        self.terms.sort_unstable_by_key(|t| t.param);
    }
}

// -------------------------------------------------------------------------------------------------
// PartialTypeSizeFormula
// -------------------------------------------------------------------------------------------------

/// A type's four size formulae over some parameters, fully resolved and flat (no pending
/// datatype applications). This is the value cached per datatype key on the vtable, and the
/// intermediate the interpreter threads while resolving a term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartialTypeSizeFormula {
    pub(crate) type_size: LinearForm,
    pub(crate) type_depth: MaxPlusForm,
    pub(crate) value_depth: MaxPlusForm,
    pub(crate) layout_size: LinearForm,
}

impl PartialTypeSizeFormula {
    /// The form of parameter `param`: every measure is that parameter's own measure.
    pub(crate) fn parameter(param: TyParamIndex) -> Self {
        Self {
            type_size: LinearForm::parameter(param),
            type_depth: MaxPlusForm::parameter(param),
            value_depth: MaxPlusForm::parameter(param),
            layout_size: LinearForm::parameter(param),
        }
    }

    /// The form of a non-composite ("primitive") type: constant `1` in every measure.
    pub(crate) fn primitive() -> Self {
        Self {
            type_size: LinearForm::constant(1),
            type_depth: MaxPlusForm::constant(1),
            value_depth: MaxPlusForm::constant(1),
            layout_size: LinearForm::constant(1),
        }
    }

    /// The form of `vector<self>` / `&self` / `&mut self`: one node and one level on top of the
    /// element in every measure.
    pub(crate) fn wrap(&self) -> Self {
        let mut type_size = LinearForm::constant(1);
        type_size.absorb(1, &self.type_size);
        let mut layout_size = LinearForm::constant(1);
        layout_size.absorb(1, &self.layout_size);
        let mut type_depth = MaxPlusForm::constant(1);
        type_depth.absorb(1, &self.type_depth);
        let mut value_depth = MaxPlusForm::constant(1);
        value_depth.absorb(1, &self.value_depth);
        Self {
            type_size,
            type_depth,
            value_depth,
            layout_size,
        }
    }

    /// Solve a parameterless (fully concrete) formula against no arguments.
    pub(crate) fn solved(&self) -> PartialVMResult<TypeSize> {
        self.solve(&[])
    }

    /// Solve against concrete argument sizes: run each measure's flat-form [`solve`](LinearForm::solve)
    /// against the matching measure of the arguments, giving a concrete [`TypeSize`].
    ///
    /// Example: for a `vector<T>` (so every measure is `1 + x0`, and depths `max(1, 1+x0)`) with
    /// `args = [T's size]` where `T = u64` (all measures `1`), each measure solves to `2` — i.e.
    /// `TypeSize` all-`2`, the size of `vector<u64>`.
    pub(crate) fn solve(&self, args: &[TypeSize]) -> PartialVMResult<TypeSize> {
        Ok(TypeSize {
            type_size: self.solve_type_size(args)?,
            type_depth: self.solve_type_depth(args)?,
            value_depth: self.solve_value_depth(args)?,
            layout_size: self.solve_layout_size(args)?,
        })
    }

    // Per-measure solves, for the call sites that only need one measure and would otherwise
    // compute (and discard) the other three.

    pub(crate) fn solve_type_size(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.type_size.solve(&project(args, |s| s.type_size))
    }

    pub(crate) fn solve_type_depth(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.type_depth.solve(&project(args, |s| s.type_depth))
    }

    pub(crate) fn solve_value_depth(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.value_depth.solve(&project(args, |s| s.value_depth))
    }

    pub(crate) fn solve_layout_size(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.layout_size.solve(&project(args, |s| s.layout_size))
    }

    /// Substitute a formula for each parameter (positionally): run each measure's flat-form
    /// [`substitute`](LinearForm::substitute) against the matching measure of the arguments. This
    /// is how a datatype instantiation `D<A0, A1, ..>` folds its argument formulae into `D`'s.
    ///
    /// Example: `D`'s `type_size` is `1 + x0`; instantiating with `A0` whose `type_size` is
    /// `1 + x1` yields `1 + (1 + x1) = 2 + x1` — a formula over the *ambient* parameters.
    pub(crate) fn substitute(
        &self,
        args: &[PartialTypeSizeFormula],
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        Ok(PartialTypeSizeFormula {
            type_size: self
                .type_size
                .substitute(&project(args, |a| a.type_size.clone()))?,
            type_depth: self
                .type_depth
                .substitute(&project(args, |a| a.type_depth.clone()))?,
            value_depth: self
                .value_depth
                .substitute(&project(args, |a| a.value_depth.clone()))?,
            layout_size: self
                .layout_size
                .substitute(&project(args, |a| a.layout_size.clone()))?,
        })
    }
}

fn project<T, U>(items: &[T], f: impl Fn(&T) -> U) -> Vec<U> {
    items.iter().map(f).collect()
}

// -------------------------------------------------------------------------------------------------
// ArenaTypeSizeFormula
// -------------------------------------------------------------------------------------------------

/// A datatype-application field of a datatype, left symbolic until a linkage resolves it.
/// `field_type` is the field's datatype-application type (`R<a…>`); `value_depth_offset` and
/// `layout_size_coeff` are how the field folds into the two through-field measures.
#[derive(Debug)]
pub(crate) struct ArenaApply {
    field_type: VMPointer<ArenaType>,
    value_depth_offset: u64,
    layout_size_coeff: u64,
}

/// A datatype's four size formulae over its own type parameters, built at JIT time.
/// `type_size`/`type_depth` are closed. `value_depth`/`layout_size` are the `*_local` part (the
/// contribution of the datatype's primitive/parameter/vector field structure) plus `apps` (the
/// datatype-application fields), resolved by [`substitute`](Self::substitute).
#[derive(Debug)]
pub(crate) struct ArenaTypeSizeFormula {
    pub(crate) type_size: LinearForm,
    pub(crate) type_depth: MaxPlusForm,
    pub(crate) value_depth_local: MaxPlusForm,
    pub(crate) layout_size_local: LinearForm,
    pub(crate) apps: ArenaVec<ArenaApply>,
}

impl ArenaTypeSizeFormula {
    /// Build a datatype's size formulae straight from its definition. A struct folds in its own
    /// fields; an enum folds in the fields of every variant and counts one extra layout node per
    /// variant.
    pub(crate) fn from_datatype(
        datatype: &Datatype,
        arena: &ArenaBuilder,
    ) -> PartialVMResult<ArenaTypeSizeFormula> {
        // Build the formulae from a datatype's type-parameter count, its (flattened) field types,
        // and its `extra_layout_nodes` (one per variant for enums, zero for structs).
        fn from_fields<'a>(
            num_params: u16,
            field_types: impl Iterator<Item = &'a ArenaType>,
            extra_layout_nodes: u64,
            arena: &ArenaBuilder,
        ) -> PartialVMResult<ArenaTypeSizeFormula> {
            // Fold one field (at `prefix_depth` value-nesting levels below the datatype) into the
            // through-field forms. `prefix_depth` starts at 1 (a direct field sits one level below
            // the datatype itself). Datatype-application fields become symbolic `ArenaApply`s.
            fn visit_field(
                ty: &ArenaType,
                prefix_depth: u64,
                value_depth_local: &mut MaxPlusForm,
                layout_size_local: &mut LinearForm,
                apps: &mut Vec<ArenaApply>,
            ) {
                match ty {
                    ArenaType::TyParam(idx) => {
                        match value_depth_local.terms.iter_mut().find(|t| t.param == *idx) {
                            Some(existing) => existing.offset = existing.offset.max(prefix_depth),
                            None => value_depth_local.terms.push(MaxPlusTerm {
                                param: *idx,
                                offset: prefix_depth,
                            }),
                        }
                        match layout_size_local.terms.iter_mut().find(|t| t.param == *idx) {
                            Some(existing) => {
                                existing.coefficient = existing.coefficient.saturating_add(1)
                            }
                            None => layout_size_local.terms.push(LinearTerm {
                                param: *idx,
                                coefficient: 1,
                            }),
                        }
                    }
                    ArenaType::Vector(inner)
                    | ArenaType::Reference(inner)
                    | ArenaType::MutableReference(inner) => {
                        value_depth_local.constant = value_depth_local
                            .constant
                            .max(prefix_depth.saturating_add(1));
                        layout_size_local.constant = layout_size_local.constant.saturating_add(1);
                        visit_field(
                            inner,
                            prefix_depth.saturating_add(1),
                            value_depth_local,
                            layout_size_local,
                            apps,
                        );
                    }
                    ArenaType::Datatype(_) | ArenaType::DatatypeInstantiation(_) => {
                        apps.push(ArenaApply {
                            field_type: VMPointer::from_ref(ty),
                            value_depth_offset: prefix_depth,
                            layout_size_coeff: 1,
                        });
                    }
                    _ => {
                        value_depth_local.constant = value_depth_local
                            .constant
                            .max(prefix_depth.saturating_add(1));
                        layout_size_local.constant = layout_size_local.constant.saturating_add(1);
                    }
                }
            }

            // The datatype instantiated over its own parameters, `S<T0..Tn>`: one node plus each
            // parameter, one level deep.
            let type_size = LinearForm {
                constant: 1,
                terms: (0..num_params)
                    .map(|param| LinearTerm {
                        param,
                        coefficient: 1,
                    })
                    .collect(),
            };
            let type_depth = MaxPlusForm {
                constant: 1,
                terms: (0..num_params)
                    .map(|param| MaxPlusTerm { param, offset: 1 })
                    .collect(),
            };

            // Through-field: the datatype contributes one value-nesting level and one layout node
            // (plus the flat overhead); each field sits one level below it.
            let mut value_depth_local = MaxPlusForm::constant(1);
            let mut layout_size_local =
                LinearForm::constant(1u64.saturating_add(extra_layout_nodes));
            let mut apps = vec![];
            for field in field_types {
                visit_field(
                    field,
                    1,
                    &mut value_depth_local,
                    &mut layout_size_local,
                    &mut apps,
                );
            }
            value_depth_local.canonicalize();
            layout_size_local.canonicalize();
            Ok(ArenaTypeSizeFormula {
                type_size,
                type_depth,
                value_depth_local,
                layout_size_local,
                apps: arena.alloc_vec(apps.into_iter())?,
            })
        }

        match datatype {
            Datatype::Struct(struct_) => {
                let struct_ = struct_.to_ref();
                from_fields(
                    checked_as!(struct_.type_parameters.len(), u16)?,
                    struct_.fields.iter(),
                    0,
                    arena,
                )
            }
            Datatype::Enum(enum_) => {
                let enum_ = enum_.to_ref();
                from_fields(
                    checked_as!(enum_.type_parameters.len(), u16)?,
                    enum_
                        .variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter()),
                    enum_.variants.len() as u64,
                    arena,
                )
            }
        }
    }

    /// Resolve this datatype's formula against a linkage (the vtable is the env), yielding a flat
    /// [`PartialTypeSizeFormula`] over the datatype's parameters. Each application is resolved by
    /// interpreting its field type against the vtable (`arena_type_size_formula`, which hits
    /// `virtual_key_size_formula` and recurs back here through the cache).
    pub(crate) fn resolve(
        &self,
        env: &VMDispatchTables,
        visiting: &mut Visiting,
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        let mut value_depth = self.value_depth_local.clone();
        let mut layout_size = self.layout_size_local.clone();
        for apply in self.apps.iter() {
            let applied = env.arena_type_size_formula_impl(apply.field_type.to_ref(), visiting)?;
            value_depth.absorb(apply.value_depth_offset, &applied.value_depth);
            layout_size.absorb(apply.layout_size_coeff, &applied.layout_size);
        }
        value_depth.canonicalize();
        layout_size.canonicalize();
        Ok(PartialTypeSizeFormula {
            type_size: self.type_size.clone(),
            type_depth: self.type_depth.clone(),
            value_depth,
            layout_size,
        })
    }
}
