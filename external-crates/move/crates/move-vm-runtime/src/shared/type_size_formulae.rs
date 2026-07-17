// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::{ArenaBuilder, ArenaVec},
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::{ArenaType, Type},
    shared::constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    partial_vm_error,
};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Type Size Formulae
// -------------------------------------------------------------------------------------------------
// The VM bounds types with four distinct quantities, each with its own limit:
//
// - `type_size`: the syntactic node count of a type term;
// - `type_depth`: the syntactic depth of a type term;
// - `value_depth`: the depth of a *value* of the type, through datatype fields;
// - `layout_size`: the node count of the type's generated layout, through datatype fields.
//
// All four used to be enforced by threading counters or runtime field traversals through every
// recursive type operation. Instead, we *predict* each quantity with a closed-form formula and
// check the prediction up front, so rejection is pure arithmetic and no part of an oversized
// type, value, or layout is ever built.
//
// Every measure lives in one of exactly two algebras, each with a flat canonical normal form
// that is closed under substitution:
//
// - additive (linear) forms, [`LinearFormula`]: `c + Σᵢ kᵢ·xᵢ` — `type_size`, `layout_size`;
// - max-plus (tropical) forms, [`MaxPlusFormula`]: `max(c, maxᵢ(dᵢ + xᵢ))` — `type_depth`,
//   `value_depth`.
//
// Substitution is same-measure: the value depth of a composite depends only on the value
// depths of its arguments, and so on. The four measures are therefore fully independent
// end-to-end, and each can be solved without computing the others.
//
// The syntactic pair is a property of a type *term* alone, so its formulas close at
// translation time. The through-field pair reaches through datatype fields, and a field may
// apply a datatype from another package whose definition is only resolvable under a
// transaction's linkage: those applications stay symbolic ([`ApplyFormula`]) in *partial*
// forms ([`PartialLinearFormula`], [`PartialMaxPlusFormula`]), built once per package version
// at translation time with their arguments pre-lowered to sub-forms. The dispatch tables close
// them per (datatype, linkage) with pure formula algebra — no arena traversal happens at link
// time or runtime. See `VMDispatchTables::size_info`.
//
// All arithmetic saturates: every quantity exists only to be compared against a limit, and a
// saturated value exceeds any limit, which is the correct verdict.

/// All four size quantities of a concrete type. These are cached per type argument on every
/// call frame (see [`TypeArguments`]), computed once when the frame is created, so every later
/// limit check against a frame's type arguments is pure arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSize {
    pub type_size: u64,
    pub type_depth: u64,
    pub value_depth: u64,
    pub layout_size: u64,
}

/// A closed additive form: `constant + Σ terms[i].1 · x_{terms[i].0}`. The formula for
/// `type_size` and `layout_size`. `terms` is sparse, sorted by parameter index, merged by
/// summing coefficients.
///
/// The container is generic so the same formula can live on the heap (`Vec`, the default —
/// products of closing and on-the-fly construction) or in a package arena (`ArenaVec`,
/// translation-time formulas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearFormula<C = Vec<(u16, u64)>> {
    pub(crate) constant: u64,
    pub(crate) terms: C,
}

/// A closed max-plus (tropical) form: `max(constant, maxᵢ(terms[i].1 + x_{terms[i].0}))`. The
/// formula for `type_depth` and `value_depth`. `terms` is sparse, sorted by parameter index,
/// merged by taking the maximum offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaxPlusFormula<C = Vec<(u16, u64)>> {
    pub(crate) constant: u64,
    pub(crate) terms: C,
}

pub(crate) type ArenaLinearFormula = LinearFormula<ArenaVec<(u16, u64)>>;
pub(crate) type ArenaMaxPlusFormula = MaxPlusFormula<ArenaVec<(u16, u64)>>;

fn missing_argument_error(param: u16, len: usize) -> PartialVMError {
    partial_vm_error!(
        UNKNOWN_INVARIANT_VIOLATION_ERROR,
        "type parameter {param} out of bounds -- len {len}"
    )
}

impl<C: AsRef<[(u16, u64)]>> LinearFormula<C> {
    /// Solve the formula with per-parameter values read out of `args` by `value_of`. Errors if
    /// the formula mentions a parameter with no argument.
    pub(crate) fn solve_with<T>(
        &self,
        args: &[T],
        value_of: impl Fn(&T) -> u64,
    ) -> PartialVMResult<u64> {
        let mut acc = self.constant;
        for (param, coeff) in self.terms.as_ref() {
            let arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?;
            acc = acc.saturating_add(coeff.saturating_mul(value_of(arg)));
        }
        Ok(acc)
    }

    pub(crate) fn solve(&self, args: &[u64]) -> PartialVMResult<u64> {
        self.solve_with(args, |x| *x)
    }

    /// Total number of type-parameter occurrences. The true node count of a substitution
    /// result is the solved prediction minus this (the prediction also counts the parameter
    /// nodes themselves, mirroring the legacy checked traversal).
    pub(crate) fn occurrences(&self) -> u64 {
        self.terms
            .as_ref()
            .iter()
            .fold(0u64, |acc, (_, coeff)| acc.saturating_add(*coeff))
    }
}

impl LinearFormula {
    pub(crate) fn constant(constant: u64) -> Self {
        Self {
            constant,
            terms: vec![],
        }
    }

    /// Add `multiplicity` copies of `other` into this formula.
    pub(crate) fn absorb<C: AsRef<[(u16, u64)]>>(
        &mut self,
        multiplicity: u64,
        other: &LinearFormula<C>,
    ) {
        self.constant = self
            .constant
            .saturating_add(multiplicity.saturating_mul(other.constant));
        for (param, coeff) in other.terms.as_ref() {
            let scaled = multiplicity.saturating_mul(*coeff);
            match self.terms.iter_mut().find(|(p, _)| p == param) {
                Some((_, acc)) => *acc = acc.saturating_add(scaled),
                None => self.terms.push((*param, scaled)),
            }
        }
    }

    /// Substitute a formula for each parameter (indexed positionally). Linear forms are closed
    /// under substitution: the result is again a flat linear form.
    pub(crate) fn subst(&self, args: &[LinearFormula]) -> PartialVMResult<LinearFormula> {
        let mut result = LinearFormula::constant(self.constant);
        for (param, coeff) in &self.terms {
            let arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?;
            result.absorb(*coeff, arg);
        }
        result.canonicalize();
        Ok(result)
    }

    pub(crate) fn canonicalize(&mut self) {
        self.terms.sort_unstable_by_key(|(param, _)| *param);
    }

    /// Move this formula's terms into `arena`, producing the arena-resident form stored in
    /// loaded packages.
    pub(crate) fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<ArenaLinearFormula> {
        Ok(LinearFormula {
            constant: self.constant,
            terms: arena.alloc_vec(self.terms.into_iter())?,
        })
    }
}

impl<C: AsRef<[(u16, u64)]>> MaxPlusFormula<C> {
    /// Solve the formula with per-parameter values read out of `args` by `value_of`. Errors if
    /// the formula mentions a parameter with no argument.
    pub(crate) fn solve_with<T>(
        &self,
        args: &[T],
        value_of: impl Fn(&T) -> u64,
    ) -> PartialVMResult<u64> {
        let mut acc = self.constant;
        for (param, offset) in self.terms.as_ref() {
            let arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?;
            acc = acc.max(offset.saturating_add(value_of(arg)));
        }
        Ok(acc)
    }

    pub(crate) fn solve(&self, args: &[u64]) -> PartialVMResult<u64> {
        self.solve_with(args, |x| *x)
    }
}

impl MaxPlusFormula {
    pub(crate) fn constant(constant: u64) -> Self {
        Self {
            constant,
            terms: vec![],
        }
    }

    /// Max `other`, shifted up by `offset`, into this formula.
    pub(crate) fn absorb<C: AsRef<[(u16, u64)]>>(
        &mut self,
        offset: u64,
        other: &MaxPlusFormula<C>,
    ) {
        self.constant = self.constant.max(offset.saturating_add(other.constant));
        for (param, arg_offset) in other.terms.as_ref() {
            let shifted = offset.saturating_add(*arg_offset);
            match self.terms.iter_mut().find(|(p, _)| p == param) {
                Some((_, acc)) => *acc = (*acc).max(shifted),
                None => self.terms.push((*param, shifted)),
            }
        }
    }

    /// Substitute a formula for each parameter (indexed positionally). Max-plus forms are
    /// closed under substitution: the result is again a flat max-plus form.
    pub(crate) fn subst(&self, args: &[MaxPlusFormula]) -> PartialVMResult<MaxPlusFormula> {
        let mut result = MaxPlusFormula::constant(self.constant);
        for (param, offset) in &self.terms {
            let arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?;
            result.absorb(*offset, arg);
        }
        result.canonicalize();
        Ok(result)
    }

    pub(crate) fn canonicalize(&mut self) {
        self.terms.sort_unstable_by_key(|(param, _)| *param);
    }

    /// Move this formula's terms into `arena`, producing the arena-resident form stored in
    /// loaded packages.
    pub(crate) fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<ArenaMaxPlusFormula> {
        Ok(MaxPlusFormula {
            constant: self.constant,
            terms: arena.alloc_vec(self.terms.into_iter())?,
        })
    }
}

/// Check a solved syntactic pair against the type-traversal limits: depth first, then size,
/// mirroring the order of the legacy checked traversal.
pub(crate) fn check_syntactic_limits(type_size: u64, type_depth: u64) -> PartialVMResult<()> {
    if type_depth > TYPE_DEPTH_MAX {
        return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
    }
    if type_size > MAX_TYPE_INSTANTIATION_NODES {
        return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------------
// Partial Formulae
// -------------------------------------------------------------------------------------------------

/// A pending datatype application inside a partial form: `key` applied to `args`, one sub-form
/// per type argument, in the same measure as the ambient form. The application's own formula
/// over its parameters is unknown until the key is resolved under a linkage; the dispatch
/// tables fold these at closing time with pure formula algebra.
#[derive(Debug)]
pub(crate) struct ApplyFormula<F> {
    pub(crate) key: VirtualTableKey,
    pub(crate) args: ArenaVec<F>,
}

/// A partial additive form: `constant + Σ params[i].1·x + Σ applies[j].0 · Apply(...)`.
#[derive(Debug)]
pub(crate) struct PartialLinearFormula {
    pub(crate) constant: u64,
    pub(crate) params: ArenaVec<(u16, u64)>,
    /// Pending applications, each with a multiplicity.
    pub(crate) applies: ArenaVec<(u64, ApplyFormula<PartialLinearFormula>)>,
}

/// A partial max-plus form: `max(constant, params[i].1 + x, applies[j].0 + Apply(...))`.
#[derive(Debug)]
pub(crate) struct PartialMaxPlusFormula {
    pub(crate) constant: u64,
    pub(crate) params: ArenaVec<(u16, u64)>,
    /// Pending applications, each with an offset.
    pub(crate) applies: ArenaVec<(u64, ApplyFormula<PartialMaxPlusFormula>)>,
}

/// Heap-side builder for [`PartialLinearFormula`], used during translation; allocated into the
/// package arena once complete.
#[derive(Debug, Default, Clone)]
pub(crate) struct PartialLinearBuilder {
    constant: u64,
    params: BTreeMap<u16, u64>,
    applies: Vec<(u64, VirtualTableKey, Vec<PartialLinearBuilder>)>,
}

/// Heap-side builder for [`PartialMaxPlusFormula`], used during translation; allocated into
/// the package arena once complete.
#[derive(Debug, Default, Clone)]
pub(crate) struct PartialMaxPlusBuilder {
    constant: u64,
    params: BTreeMap<u16, u64>,
    applies: Vec<(u64, VirtualTableKey, Vec<PartialMaxPlusBuilder>)>,
}

impl PartialLinearBuilder {
    fn from_partial(partial: &PartialLinearFormula) -> Self {
        Self {
            constant: partial.constant,
            params: partial.params.iter().copied().collect(),
            applies: partial
                .applies
                .iter()
                .map(|(multiplicity, apply)| {
                    (
                        *multiplicity,
                        apply.key.clone(),
                        apply.args.iter().map(Self::from_partial).collect(),
                    )
                })
                .collect(),
        }
    }

    /// Multiply the whole form by `factor`: the constant, every coefficient, and every pending
    /// application's multiplicity.
    fn scale(&mut self, factor: u64) {
        self.constant = self.constant.saturating_mul(factor);
        for coeff in self.params.values_mut() {
            *coeff = coeff.saturating_mul(factor);
        }
        for (multiplicity, _, _) in self.applies.iter_mut() {
            *multiplicity = multiplicity.saturating_mul(factor);
        }
    }

    /// Add `other` into this form.
    fn add(&mut self, other: PartialLinearBuilder) {
        self.constant = self.constant.saturating_add(other.constant);
        for (param, coeff) in other.params {
            let entry = self.params.entry(param).or_insert(0);
            *entry = entry.saturating_add(coeff);
        }
        self.applies.extend(other.applies);
    }

    /// Substitute `args` (forms over the ambient parameters) for this form's parameters.
    /// Partial linear forms are closed under substitution — pending applications keep their
    /// keys, with their argument sub-forms substituted recursively.
    fn subst(&self, args: &[PartialLinearBuilder]) -> PartialVMResult<PartialLinearBuilder> {
        let mut out = PartialLinearBuilder {
            constant: self.constant,
            params: BTreeMap::new(),
            applies: vec![],
        };
        for (param, coeff) in &self.params {
            let mut arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?
                .clone();
            arg.scale(*coeff);
            out.add(arg);
        }
        for (multiplicity, key, apply_args) in &self.applies {
            let apply_args = apply_args
                .iter()
                .map(|arg| arg.subst(args))
                .collect::<PartialVMResult<Vec<_>>>()?;
            out.applies.push((*multiplicity, key.clone(), apply_args));
        }
        Ok(out)
    }

    fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<PartialLinearFormula> {
        let PartialLinearBuilder {
            constant,
            params,
            applies,
        } = self;
        let applies = applies
            .into_iter()
            .map(|(multiplicity, key, args)| {
                let args = args
                    .into_iter()
                    .map(|arg| arg.allocate(arena))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                Ok((
                    multiplicity,
                    ApplyFormula {
                        key,
                        args: arena.alloc_vec(args.into_iter())?,
                    },
                ))
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok(PartialLinearFormula {
            constant,
            params: arena.alloc_vec(params.into_iter())?,
            applies: arena.alloc_vec(applies.into_iter())?,
        })
    }
}

impl PartialMaxPlusBuilder {
    fn from_partial(partial: &PartialMaxPlusFormula) -> Self {
        Self {
            constant: partial.constant,
            params: partial.params.iter().copied().collect(),
            applies: partial
                .applies
                .iter()
                .map(|(offset, apply)| {
                    (
                        *offset,
                        apply.key.clone(),
                        apply.args.iter().map(Self::from_partial).collect(),
                    )
                })
                .collect(),
        }
    }

    /// Shift the whole form up by `delta`: the constant, every parameter offset, and every
    /// pending application's offset.
    fn shift(&mut self, delta: u64) {
        self.constant = self.constant.saturating_add(delta);
        for offset in self.params.values_mut() {
            *offset = offset.saturating_add(delta);
        }
        for (offset, _, _) in self.applies.iter_mut() {
            *offset = offset.saturating_add(delta);
        }
    }

    /// Max `other` into this form (offsets are already absolute).
    fn merge_max(&mut self, other: PartialMaxPlusBuilder) {
        self.constant = self.constant.max(other.constant);
        for (param, offset) in other.params {
            let entry = self.params.entry(param).or_insert(0);
            *entry = (*entry).max(offset);
        }
        self.applies.extend(other.applies);
    }

    /// Substitute `args` (forms over the ambient parameters) for this form's parameters.
    /// Partial max-plus forms are closed under substitution — pending applications keep their
    /// keys, with their argument sub-forms substituted recursively.
    fn subst(&self, args: &[PartialMaxPlusBuilder]) -> PartialVMResult<PartialMaxPlusBuilder> {
        let mut out = PartialMaxPlusBuilder {
            constant: self.constant,
            params: BTreeMap::new(),
            applies: vec![],
        };
        for (param, offset) in &self.params {
            let mut arg = args
                .get(*param as usize)
                .ok_or_else(|| missing_argument_error(*param, args.len()))?
                .clone();
            arg.shift(*offset);
            out.merge_max(arg);
        }
        for (offset, key, apply_args) in &self.applies {
            let apply_args = apply_args
                .iter()
                .map(|arg| arg.subst(args))
                .collect::<PartialVMResult<Vec<_>>>()?;
            out.applies.push((*offset, key.clone(), apply_args));
        }
        Ok(out)
    }

    fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<PartialMaxPlusFormula> {
        let PartialMaxPlusBuilder {
            constant,
            params,
            applies,
        } = self;
        let applies = applies
            .into_iter()
            .map(|(offset, key, args)| {
                let args = args
                    .into_iter()
                    .map(|arg| arg.allocate(arena))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                Ok((
                    offset,
                    ApplyFormula {
                        key,
                        args: arena.alloc_vec(args.into_iter())?,
                    },
                ))
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok(PartialMaxPlusFormula {
            constant,
            params: arena.alloc_vec(params.into_iter())?,
            applies: arena.alloc_vec(applies.into_iter())?,
        })
    }
}

// -------------------------------------------------------------------------------------------------
// Intra-Package Folding
// -------------------------------------------------------------------------------------------------

/// The (possibly folded) through-field size information of a datatype, in heap-builder form,
/// as consulted while building other formulas at translation time.
#[derive(Debug, Clone)]
pub(crate) enum FoldedSizeInfo {
    Constant(DatatypeSizes),
    Formula {
        value_depth: PartialMaxPlusBuilder,
        layout_size: PartialLinearBuilder,
    },
}

impl FoldedSizeInfo {
    /// Bundle the builder pair, collapsing to constants when nothing symbolic remains.
    pub(crate) fn from_builders(
        value_depth: PartialMaxPlusBuilder,
        layout_size: PartialLinearBuilder,
    ) -> Self {
        let concrete = value_depth.params.is_empty()
            && value_depth.applies.is_empty()
            && layout_size.params.is_empty()
            && layout_size.applies.is_empty();
        if concrete {
            FoldedSizeInfo::Constant(DatatypeSizes {
                value_depth: value_depth.constant,
                layout_size: layout_size.constant,
            })
        } else {
            FoldedSizeInfo::Formula {
                value_depth,
                layout_size,
            }
        }
    }

    pub(crate) fn from_size_info(info: &DatatypeSizeInfo) -> Self {
        match info {
            DatatypeSizeInfo::Constant(sizes) => FoldedSizeInfo::Constant(*sizes),
            DatatypeSizeInfo::Formula {
                value_depth,
                layout_size,
            } => FoldedSizeInfo::Formula {
                value_depth: PartialMaxPlusBuilder::from_partial(value_depth),
                layout_size: PartialLinearBuilder::from_partial(layout_size),
            },
        }
    }

    /// Allocate into `arena` as descriptor-resident size information, collapsing to constants
    /// when nothing symbolic remains.
    pub(crate) fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<DatatypeSizeInfo> {
        match self {
            FoldedSizeInfo::Constant(sizes) => Ok(DatatypeSizeInfo::Constant(sizes)),
            FoldedSizeInfo::Formula {
                value_depth,
                layout_size,
            } => match FoldedSizeInfo::from_builders(value_depth, layout_size) {
                FoldedSizeInfo::Constant(sizes) => Ok(DatatypeSizeInfo::Constant(sizes)),
                FoldedSizeInfo::Formula {
                    value_depth,
                    layout_size,
                } => Ok(DatatypeSizeInfo::Formula {
                    value_depth: value_depth.allocate(arena)?,
                    layout_size: layout_size.allocate(arena)?,
                }),
            },
        }
    }
}

/// Resolves datatype applications while building through-field forms at translation time.
/// Returning `Some` folds the application eagerly (intra-package references, whose resolution
/// is linkage-independent — a package's self-references always resolve to itself); returning
/// `None` leaves it as a pending [`ApplyFormula`] for the dispatch tables to close under a
/// transaction's linkage (cross-package references).
pub(crate) trait ApplyResolver {
    fn resolve_apply(&mut self, key: &VirtualTableKey) -> PartialVMResult<Option<FoldedSizeInfo>>;
}

/// Fold nothing: every datatype application stays symbolic. (Production translation always
/// folds intra-package applications; this is for building formulas in isolation in tests.)
#[cfg(test)]
pub(crate) struct NoFolding;

#[cfg(test)]
impl ApplyResolver for NoFolding {
    fn resolve_apply(&mut self, _key: &VirtualTableKey) -> PartialVMResult<Option<FoldedSizeInfo>> {
        Ok(None)
    }
}

/// The through-field `layout_size` form of a type term: one layout node per structural node,
/// datatype applications folded through `resolver` or kept pending with their argument
/// sub-forms pre-lowered.
fn layout_form_of_term<R: ApplyResolver>(
    ty: &ArenaType,
    resolver: &mut R,
) -> PartialVMResult<PartialLinearBuilder> {
    Ok(match ty {
        ArenaType::TyParam(idx) => PartialLinearBuilder {
            constant: 0,
            params: BTreeMap::from([(*idx, 1)]),
            applies: vec![],
        },
        ArenaType::Vector(inner)
        | ArenaType::Reference(inner)
        | ArenaType::MutableReference(inner) => {
            let mut form = layout_form_of_term(inner, resolver)?;
            form.constant = form.constant.saturating_add(1);
            form
        }
        ArenaType::Datatype(key) => layout_apply(key, vec![], resolver)?,
        ArenaType::DatatypeInstantiation(inst) => {
            let (key, ty_args) = &**inst;
            let args = ty_args
                .iter()
                .map(|arg| layout_form_of_term(arg, resolver))
                .collect::<PartialVMResult<Vec<_>>>()?;
            layout_apply(key, args, resolver)?
        }
        _ => PartialLinearBuilder {
            constant: 1,
            params: BTreeMap::new(),
            applies: vec![],
        },
    })
}

fn layout_apply<R: ApplyResolver>(
    key: &VirtualTableKey,
    args: Vec<PartialLinearBuilder>,
    resolver: &mut R,
) -> PartialVMResult<PartialLinearBuilder> {
    Ok(match resolver.resolve_apply(key)? {
        // A fully concrete application contributes its layout size outright; its formulas
        // mention no parameters (e.g. phantoms), so the arguments contribute nothing.
        Some(FoldedSizeInfo::Constant(sizes)) => PartialLinearBuilder {
            constant: sizes.layout_size,
            params: BTreeMap::new(),
            applies: vec![],
        },
        Some(FoldedSizeInfo::Formula { layout_size, .. }) => layout_size.subst(&args)?,
        None => PartialLinearBuilder {
            constant: 0,
            params: BTreeMap::new(),
            applies: vec![(1, key.clone(), args)],
        },
    })
}

/// The through-field `value_depth` form of a type term: one value-nesting level per structural
/// node, datatype applications folded through `resolver` or kept pending with their argument
/// sub-forms pre-lowered.
fn value_form_of_term<R: ApplyResolver>(
    ty: &ArenaType,
    resolver: &mut R,
) -> PartialVMResult<PartialMaxPlusBuilder> {
    Ok(match ty {
        ArenaType::TyParam(idx) => PartialMaxPlusBuilder {
            constant: 0,
            params: BTreeMap::from([(*idx, 0)]),
            applies: vec![],
        },
        ArenaType::Vector(inner)
        | ArenaType::Reference(inner)
        | ArenaType::MutableReference(inner) => {
            let mut form = value_form_of_term(inner, resolver)?;
            form.shift(1);
            form
        }
        ArenaType::Datatype(key) => value_apply(key, vec![], resolver)?,
        ArenaType::DatatypeInstantiation(inst) => {
            let (key, ty_args) = &**inst;
            let args = ty_args
                .iter()
                .map(|arg| value_form_of_term(arg, resolver))
                .collect::<PartialVMResult<Vec<_>>>()?;
            value_apply(key, args, resolver)?
        }
        _ => PartialMaxPlusBuilder {
            constant: 1,
            params: BTreeMap::new(),
            applies: vec![],
        },
    })
}

fn value_apply<R: ApplyResolver>(
    key: &VirtualTableKey,
    args: Vec<PartialMaxPlusBuilder>,
    resolver: &mut R,
) -> PartialVMResult<PartialMaxPlusBuilder> {
    Ok(match resolver.resolve_apply(key)? {
        // A fully concrete application contributes its value depth outright; its formulas
        // mention no parameters (e.g. phantoms), so the arguments contribute nothing.
        Some(FoldedSizeInfo::Constant(sizes)) => PartialMaxPlusBuilder {
            constant: sizes.value_depth,
            params: BTreeMap::new(),
            applies: vec![],
        },
        Some(FoldedSizeInfo::Formula { value_depth, .. }) => value_depth.subst(&args)?,
        None => PartialMaxPlusBuilder {
            constant: 0,
            params: BTreeMap::new(),
            applies: vec![(0, key.clone(), args)],
        },
    })
}

/// The through-field builder pair for a datatype with the given field types (for enums, the
/// fields of every variant). See [`DatatypeSizeInfo::for_datatype_fields`].
pub(crate) fn datatype_builders_for_fields<'a, R: ApplyResolver>(
    field_types: impl Iterator<Item = &'a ArenaType>,
    extra_layout_nodes: u64,
    resolver: &mut R,
) -> PartialVMResult<(PartialMaxPlusBuilder, PartialLinearBuilder)> {
    // The datatype itself contributes one value-nesting level and one layout node (plus the
    // flat overhead); each field sits one level below the datatype.
    let mut value = PartialMaxPlusBuilder {
        constant: 1,
        params: BTreeMap::new(),
        applies: vec![],
    };
    let mut layout = PartialLinearBuilder {
        constant: 1u64.saturating_add(extra_layout_nodes),
        params: BTreeMap::new(),
        applies: vec![],
    };
    for field_ty in field_types {
        let mut field_value = value_form_of_term(field_ty, resolver)?;
        field_value.shift(1);
        value.merge_max(field_value);

        let field_layout = layout_form_of_term(field_ty, resolver)?;
        layout.add(field_layout);
    }
    Ok((value, layout))
}

// -------------------------------------------------------------------------------------------------
// Per-Term Formulae// -------------------------------------------------------------------------------------------------
// Per-Term Formulae
// -------------------------------------------------------------------------------------------------

/// A signature-pool type term together with all four of its size formulas, computed once at
/// translation time. The syntactic pair is closed (a datatype head is a single syntactic
/// node, so no linkage is needed); the through-field pair is partial, closed by the dispatch
/// tables under a transaction's linkage.
///
/// This is plain data: the operations — checked substitution, instantiation checks — live on
/// the dispatch tables (`subst_type`, `check_instantiation`, ...), which do the formula work
/// first and only then realize a type from `term`, if one is needed at all.
#[derive(Debug)]
pub(crate) struct PartialTypeFormula {
    pub(crate) term: ArenaType,
    pub(crate) type_size: ArenaLinearFormula,
    pub(crate) type_depth: ArenaMaxPlusFormula,
    pub(crate) value_depth: PartialMaxPlusFormula,
    pub(crate) layout_size: PartialLinearFormula,
}

impl PartialTypeFormula {
    /// Compute all four formulas for `term`, allocating them in `arena`. Intra-package
    /// datatype applications are folded eagerly through `resolver`.
    pub(crate) fn for_term<R: ApplyResolver>(
        term: ArenaType,
        arena: &ArenaBuilder,
        resolver: &mut R,
    ) -> PartialVMResult<Self> {
        let (type_size, type_depth) = term.syntactic_formulas();
        let value_depth = value_form_of_term(&term, resolver)?.allocate(arena)?;
        let layout_size = layout_form_of_term(&term, resolver)?.allocate(arena)?;
        Ok(Self {
            term,
            type_size: type_size.allocate(arena)?,
            type_depth: type_depth.allocate(arena)?,
            value_depth,
            layout_size,
        })
    }
}

// -------------------------------------------------------------------------------------------------
// Datatype (Through-Field) Formulae
// -------------------------------------------------------------------------------------------------

/// The through-field sizes of a fully concrete datatype: the maximum nesting depth of a value
/// of the type, and the node count of its generated layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatatypeSizes {
    pub(crate) value_depth: u64,
    pub(crate) layout_size: u64,
}

/// The through-field size information of a datatype, living on its [`DatatypeDescriptor`],
/// computed while the package is JIT'd. When the datatype is fully concrete — no type
/// parameters and no datatype references in its fields — the quantities are known exactly at
/// translation time and are written down as plain constants; otherwise they are partial
/// formulas over the type parameters and the (linkage-dependent) datatype applications in the
/// fields.
#[derive(Debug)]
pub(crate) enum DatatypeSizeInfo {
    Constant(DatatypeSizes),
    Formula {
        value_depth: PartialMaxPlusFormula,
        layout_size: PartialLinearFormula,
    },
}

impl DatatypeSizeInfo {
    /// Compute the through-field size information for a datatype with the given field types
    /// (for enums, the fields of every variant), allocating any formula terms in `arena`.
    /// `extra_layout_nodes` is the datatype's flat layout overhead beyond its own node — one
    /// per variant for enums, zero for structs — mirroring the per-variant node the legacy
    /// layout traversal counted. Intra-package datatype applications are folded eagerly
    /// through `resolver`. (Production translation drives [`datatype_builders_for_fields`]
    /// through the package folder directly; this is the standalone form used in tests.)
    #[cfg(test)]
    pub(crate) fn for_datatype_fields<'a, R: ApplyResolver>(
        field_types: impl Iterator<Item = &'a ArenaType>,
        extra_layout_nodes: u64,
        arena: &ArenaBuilder,
        resolver: &mut R,
    ) -> PartialVMResult<DatatypeSizeInfo> {
        let (value_depth, layout_size) =
            datatype_builders_for_fields(field_types, extra_layout_nodes, resolver)?;
        FoldedSizeInfo::Formula {
            value_depth,
            layout_size,
        }
        .allocate(arena)
    }
}

/// Linkage-resolved through-field formulas of a datatype or type term, produced by the
/// dispatch tables closing partial forms under a transaction's linkage view and memoized per
/// (datatype, linkage) / (term, linkage). Closed: the runtime solve path cannot encounter an
/// unresolved key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SizeFormula {
    pub(crate) value_depth: MaxPlusFormula,
    pub(crate) layout_size: LinearFormula,
}

impl SizeFormula {
    /// A fully concrete datatype's formulas.
    pub(crate) fn constant(sizes: DatatypeSizes) -> Self {
        Self {
            value_depth: MaxPlusFormula::constant(sizes.value_depth),
            layout_size: LinearFormula::constant(sizes.layout_size),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Type Arguments
// -------------------------------------------------------------------------------------------------

/// Fully-instantiated type arguments paired with all four of their size quantities, computed
/// once at construction (when a call frame is created) and passed down with the frame. Every
/// later limit check against these arguments is pure arithmetic. The pairing is private so the
/// sizes can never drift from the types.
#[derive(Debug, Clone)]
pub struct TypeArguments {
    types: Vec<Type>,
    sizes: Vec<TypeSize>,
}

impl TypeArguments {
    /// Pair `types` with their sizes. `sizes_of` computes the quartet for each type — the
    /// dispatch tables provide this (the through-field quantities need datatype resolution
    /// under the transaction's linkage view); see `VMDispatchTables::make_type_arguments`.
    pub(crate) fn new(
        types: Vec<Type>,
        mut sizes_of: impl FnMut(&Type) -> PartialVMResult<TypeSize>,
    ) -> PartialVMResult<Self> {
        let sizes = types
            .iter()
            .map(&mut sizes_of)
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok(Self { types, sizes })
    }

    /// Pair `types` with sizes that were computed alongside them (e.g. solved from formulas
    /// during generic-function instantiation).
    pub(crate) fn from_parts(types: Vec<Type>, sizes: Vec<TypeSize>) -> PartialVMResult<Self> {
        if types.len() != sizes.len() {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "type argument sizes mismatch: {} types, {} sizes",
                types.len(),
                sizes.len()
            ));
        }
        Ok(Self { types, sizes })
    }

    pub fn empty() -> Self {
        Self {
            types: vec![],
            sizes: vec![],
        }
    }

    pub fn types(&self) -> &[Type] {
        &self.types
    }

    pub fn sizes(&self) -> &[TypeSize] {
        &self.sizes
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}
