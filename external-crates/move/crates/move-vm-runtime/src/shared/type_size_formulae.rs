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
//! Each quantity lives in one of two algebras: additive ([`LinearForm`], for `type_size` and
//! `layout_size`) and max-plus ([`MaxPlusForm`], for `type_depth` and `value_depth`). Both are
//! flat and closed under substitution. The pipeline is a partial evaluator built from three
//! operations: **evaluate**, **substitute**, and **solve**.
//!
//! 1. **JIT** builds an [`ArenaTypeSizeFormula`] per datatype: the four forms over the
//!    datatype's own type parameters. `type_size`/`type_depth` are closed; `value_depth`/
//!    `layout_size` additionally depend on the datatype's field *applications*, which need a
//!    linkage to resolve. The application trees of every field are **linearized** at JIT time
//!    into one dependency-ordered sequence ([`Application`]), and the datatypes they mention
//!    are listed flat ([`ArenaTypeSizeFormula::vtable_keys`]) -- no type walk is ever needed to
//!    discover or resolve them. Type *terms* (instruction operands and instantiation signature
//!    entries) compile to the same shape ([`ArenaTypeSizeFormula::from_term`]), with their
//!    syntactic forms closed entirely at JIT.
//! 2. **Evaluate** ([`ArenaTypeSizeFormula::evaluate`]) closes the formula against a linkage:
//!    the dispatch tables resolve the datatype dependency graph as an iterative, cache-backed
//!    work queue (`VMDispatchTables::virtual_key_size_formula`), then the linearized
//!    applications are replayed in one pass, one **substitute**
//!    ([`PartialTypeSizeFormula::substitute`]) per entry, folding field roots into the
//!    through-field measures inline. This yields the datatype's (or term's) flat
//!    [`PartialTypeSizeFormula`].
//! 3. **Solve** ([`PartialTypeSizeFormula::solve`]) evaluates a flat form against concrete
//!    argument [`TypeSize`]s, yielding a concrete [`TypeSize`].
//!
//! All four forms are used for various safety checks across VM execution. All arithmetic
//! saturates: every quantity exists only to be compared against a limit, and a saturated value
//! exceeds any limit -- the correct verdict.

use crate::{
    cache::arena::{ArenaBuilder, ArenaVec},
    execution::dispatch_tables::{TypeCache, VirtualTableKey},
    jit::execution::ast::{ArenaType, Datatype},
    shared::constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
};
use move_binary_format::{
    checked_as,
    errors::{PartialVMError, PartialVMResult},
    partial_vm_error,
};

/// Index of a type parameter.
pub(crate) type TyParamIndex = u16;

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
/// datatype applications): the result of closing any [`ArenaTypeSizeFormula`] (datatype or
/// term) under a linkage. This is the value cached per datatype key during execution.
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
    /// `args = [T's size]` where `T = u64` (all measures `1`), each measure solves to `2` -- i.e.
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
    /// `1 + x1` yields `1 + (1 + x1) = 2 + x1` -- a formula over the *ambient* parameters.
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

/// Index into [`ArenaTypeSizeFormula::keys`].
pub(crate) type KeyIndex = u16;
/// Index into [`ArenaTypeSizeFormula::applications`].
pub(crate) type ApplicationIndex = u16;

/// One datatype application `D<a0, …, an>` from a field or term type. Evaluating it is a single
/// [`substitute`](PartialTypeSizeFormula::substitute) of the argument formulae into `D`'s
/// resolved formula.
///
/// Example: in `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }` (`T`, `R`, `S`
/// datatypes), `y`'s `T<u64>` is `{ datatype: T, arguments: [u64], .. }`, and `z`'s `R<S<A>>`
/// is `{ datatype: R, arguments: [r1], .. }`, where `r1` references `S<A>`'s own entry,
/// emitted earlier.
#[derive(Debug)]
pub(crate) struct Application {
    /// The applied datatype, as an index into [`ArenaTypeSizeFormula::keys`].
    datatype: KeyIndex,
    arguments: ArenaVec<Argument>,
    /// `Some(depth)` iff this application is one of the datatype's fields (or a term's root),
    /// with the field's value sitting `depth` nesting levels below the datatype; `None` for a
    /// subterm of a later entry, whose size is already counted inside its consumer (folding it
    /// again would double-count).
    ///
    /// Example: in `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }`, `y`'s `T<u64>`
    /// entry gets `Some(1)` (a direct field), `z`'s `R<..>` entry gets `Some(2)` (one vector
    /// layer down), and the `S<A>` inside it gets `None` -- `R`'s entry already counts it.
    field_depth: Option<u64>,
}

/// One type argument of an [`Application`]: a base type under some number of `vector<…>` (or
/// reference) layers.
///
/// Example: `vector<vector<u64>>` is `{ vector_layers: 2, base: Primitive }`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Argument {
    /// Number of `vector<…>`/reference layers around `base`; each contributes one node and one
    /// level to every measure (one [`wrap`](PartialTypeSizeFormula::wrap) at evaluation).
    vector_layers: u16,
    base: ArgumentBase,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ArgumentBase {
    /// A non-composite type (`u64`, `address`, …): constant `1` in every measure.
    Primitive,
    /// The enclosing datatype's (or, for a term, function's) type parameter `i`: the identity
    /// formula `xi`. This is what
    /// makes evaluation produce a *formula* over the datatype's parameters rather than a number.
    TypeParameter(TyParamIndex),
    /// The result of an earlier application -- how nesting reads after linearization: for
    /// datatypes `R` and `S`, in `R<S<u64>>` the entry for `S<u64>` precedes `R`'s, which
    /// references it here.
    Application(ApplicationIndex),
}

/// A datatype's four size formulae over its own type parameters, built at JIT time. (A type
/// *term* compiles to the same shape via [`from_term`](Self::from_term), over the enclosing
/// function's type parameters; "field" below then means the term's root.)
///
/// `type_size`/`type_depth` are closed. `value_depth`/`layout_size` are the `*_local` part (the
/// contribution of the datatype's primitive/parameter/vector field structure) plus the
/// datatype-application fields, carried symbolically until a linkage resolves them: given the
/// resolved formula of every datatype in `keys`, [`evaluate`](Self::evaluate) computes this
/// formula's own flat form directly.
///
/// Example: `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }` (`T`, `R`, `S` datatypes)
/// builds
///
/// ```text
/// keys         = [T, S, R]                       // first-mention order
/// applications = [ T<u64>  field_depth: Some(1), // field `y`
///                  S<x0>,                        // subterm: argument of the next entry
///                  R<r1>   field_depth: Some(2)] // field `z`, below its vector layer
/// ```
///
/// while `x` and `z`'s vector fold into the locals: `value_depth_local = max(2)` and
/// `layout_size_local = 3` (the `W` node, the `u64`, the vector).
#[derive(Debug)]
pub(crate) struct ArenaTypeSizeFormula {
    pub(crate) type_size: LinearForm,
    pub(crate) type_depth: MaxPlusForm,
    pub(crate) value_depth_local: MaxPlusForm,
    pub(crate) layout_size_local: LinearForm,
    /// Every datatype mentioned by the field (or term) types (deduplicated, first-mention
    /// order). These are exactly the formulae [`evaluate`](Self::evaluate) must be handed; the
    /// dispatch tables' resolution work queue reads this list directly, so dependency
    /// discovery never touches a type.
    keys: ArenaVec<VirtualTableKey>,
    /// The fields' (or term's) datatype applications, **linearized**: the application trees,
    /// flattened at JIT time into one sequence in dependency order -- an entry's arguments only
    /// reference the results of strictly earlier entries, so evaluation is one in-order pass
    /// with no recursion. Every syntactic application occurrence is its own entry (no
    /// common-subexpression sharing), which is what lets each entry carry its `field_depth`
    /// role inline.
    applications: ArenaVec<Application>,
}

/// JIT-side linearizer state: the deduplicated key list and the applications emitted so far
/// (heap-side builders, arena-allocated by [`finish`](Linearizer::finish) once emission is
/// complete).
struct Linearizer {
    keys: Vec<VirtualTableKey>,
    applications: Vec<ApplicationBuilder>,
}

struct ApplicationBuilder {
    datatype: KeyIndex,
    arguments: Vec<Argument>,
    field_depth: Option<u64>,
}

impl Linearizer {
    fn new() -> Self {
        Self {
            keys: vec![],
            applications: vec![],
        }
    }

    /// The index of `key` in the deduplicated key list, appending it on first mention.
    fn intern_key(&mut self, key: &VirtualTableKey) -> PartialVMResult<KeyIndex> {
        let ndx = match self.keys.iter().position(|k| k == key) {
            Some(ndx) => ndx,
            None => {
                self.keys.push(key.clone());
                self.keys.len().saturating_sub(1)
            }
        };
        checked_as!(ndx, u16)
    }

    /// Emit the applications of a datatype-application type in post-order (arguments before the
    /// application that consumes them -- the linearization invariant) and return the root's
    /// index. Runs on an explicit work stack: nothing recurs, however deeply the type nests.
    ///
    /// Example: for datatypes `R` and `S` and enclosing parameter `A`, `R<S<A>>` emits `S<x0>`
    /// then `R<r0>` and returns `R`'s index; the argument `vector<vector<A>>` compiles to
    /// `{ vector_layers: 2, base: TypeParameter(A) }`.
    fn emit_application(&mut self, ty: &ArenaType) -> PartialVMResult<ApplicationIndex> {
        // One work item: compile a type into an `Argument`, or assemble an application from
        // the last `argc` compiled arguments (its own, in order).
        enum Item<'a> {
            Compile(&'a ArenaType),
            Assemble {
                key: &'a VirtualTableKey,
                argc: usize,
                vector_layers: u16,
            },
        }

        let mut work = vec![Item::Compile(ty)];
        let mut compiled: Vec<Argument> = vec![];
        while let Some(item) = work.pop() {
            match item {
                Item::Compile(mut ty) => {
                    // Strip the vector/reference layers, then dispatch on the base.
                    let mut vector_layers: usize = 0;
                    while let ArenaType::Vector(inner)
                    | ArenaType::Reference(inner)
                    | ArenaType::MutableReference(inner) = ty
                    {
                        vector_layers = vector_layers.saturating_add(1);
                        ty = inner;
                    }
                    let vector_layers = checked_as!(vector_layers, u16)?;
                    match ty {
                        ArenaType::TyParam(idx) => compiled.push(Argument {
                            vector_layers,
                            base: ArgumentBase::TypeParameter(*idx),
                        }),
                        ArenaType::Datatype(key) => work.push(Item::Assemble {
                            key,
                            argc: 0,
                            vector_layers,
                        }),
                        ArenaType::DatatypeInstantiation(inst) => {
                            let (key, args) = &**inst;
                            work.push(Item::Assemble {
                                key,
                                argc: args.len(),
                                vector_layers,
                            });
                            // Reverse-pushed so the arguments compile (and land in
                            // `compiled`) in argument order.
                            for arg in args.iter().rev() {
                                work.push(Item::Compile(arg));
                            }
                        }
                        _ => compiled.push(Argument {
                            vector_layers,
                            base: ArgumentBase::Primitive,
                        }),
                    }
                }
                Item::Assemble {
                    key,
                    argc,
                    vector_layers,
                } => {
                    let split = compiled.len().checked_sub(argc).ok_or_else(|| {
                        partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "argument underflow while linearizing a size formula"
                        )
                    })?;
                    let arguments = compiled.split_off(split);
                    let datatype = self.intern_key(key)?;
                    let ndx = self.applications.len();
                    self.applications.push(ApplicationBuilder {
                        datatype,
                        arguments,
                        field_depth: None,
                    });
                    compiled.push(Argument {
                        vector_layers,
                        base: ArgumentBase::Application(checked_as!(ndx, u16)?),
                    });
                }
            }
        }
        // The caller hands us an application type, so exactly one bare application remains.
        match compiled.as_slice() {
            [
                Argument {
                    vector_layers: 0,
                    base: ArgumentBase::Application(root),
                },
            ] => Ok(*root),
            _ => Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "emit_application on a non-application type"
            )),
        }
    }

    /// Arena-allocate the emitted keys and applications.
    fn finish(
        self,
        arena: &ArenaBuilder,
    ) -> PartialVMResult<(ArenaVec<VirtualTableKey>, ArenaVec<Application>)> {
        let Linearizer { keys, applications } = self;
        let applications = applications
            .into_iter()
            .map(|builder| {
                Ok(Application {
                    datatype: builder.datatype,
                    arguments: arena.alloc_vec(builder.arguments.into_iter())?,
                    field_depth: builder.field_depth,
                })
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok((
            arena.alloc_vec(keys.into_iter())?,
            arena.alloc_vec(applications.into_iter())?,
        ))
    }
}

/// Fold one field (at `prefix_depth` value-nesting levels below the datatype; 1 for a direct
/// field of a datatype, 0 for a bare term) into the through-field forms. Interior vector layers
/// are consumed by the loop; datatype applications are linearized into `Application` entries
/// with the field root marked by its `field_depth`. Nothing recurs.
///
/// Example: for `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }` (`T`, `R`, `S`
/// datatypes), `x` bumps `value_depth_local`'s constant to 2 and adds one layout node; `y`
/// emits its application at depth 1 and adds nothing local; `z`'s vector adds one more of each
/// and its `R<S<A>>` is emitted at depth 2. Had `W` had a field `A` instead, it would add the
/// terms `1 + x0` (depth) and `x0` (layout).
fn visit_field(
    mut ty: &ArenaType,
    mut prefix_depth: u64,
    value_depth_local: &mut MaxPlusForm,
    layout_size_local: &mut LinearForm,
    linearizer: &mut Linearizer,
) -> PartialVMResult<()> {
    loop {
        match ty {
            ArenaType::Vector(inner)
            | ArenaType::Reference(inner)
            | ArenaType::MutableReference(inner) => {
                value_depth_local.constant = value_depth_local
                    .constant
                    .max(prefix_depth.saturating_add(1));
                layout_size_local.constant = layout_size_local.constant.saturating_add(1);
                prefix_depth = prefix_depth.saturating_add(1);
                ty = inner;
            }
            ArenaType::TyParam(idx) => {
                match value_depth_local.terms.iter_mut().find(|t| t.param == *idx) {
                    Some(existing) => existing.offset = existing.offset.max(prefix_depth),
                    None => value_depth_local.terms.push(MaxPlusTerm {
                        param: *idx,
                        offset: prefix_depth,
                    }),
                }
                match layout_size_local.terms.iter_mut().find(|t| t.param == *idx) {
                    Some(existing) => existing.coefficient = existing.coefficient.saturating_add(1),
                    None => layout_size_local.terms.push(LinearTerm {
                        param: *idx,
                        coefficient: 1,
                    }),
                }
                return Ok(());
            }
            ArenaType::Datatype(_) | ArenaType::DatatypeInstantiation(_) => {
                let root = linearizer.emit_application(ty)?;
                linearizer
                    .applications
                    .get_mut(root as usize)
                    .ok_or_else(|| {
                        partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "linearizer returned an out-of-bounds application index"
                        )
                    })?
                    .field_depth = Some(prefix_depth);
                return Ok(());
            }
            _ => {
                value_depth_local.constant = value_depth_local
                    .constant
                    .max(prefix_depth.saturating_add(1));
                layout_size_local.constant = layout_size_local.constant.saturating_add(1);
                return Ok(());
            }
        }
    }
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
            let mut linearizer = Linearizer::new();
            for field in field_types {
                visit_field(
                    field,
                    1,
                    &mut value_depth_local,
                    &mut layout_size_local,
                    &mut linearizer,
                )?;
            }
            value_depth_local.canonicalize();
            layout_size_local.canonicalize();
            let (keys, applications) = linearizer.finish(arena)?;
            Ok(ArenaTypeSizeFormula {
                type_size,
                type_depth,
                value_depth_local,
                layout_size_local,
                keys,
                applications,
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

    /// Build the size formulae of a type *term* (an instruction operand or an instantiation
    /// signature entry) over the enclosing function's type parameters. A term compiles exactly
    /// like a single datatype field at nesting depth 0 (same locals fold, same linearized
    /// applications); only `type_size`/`type_depth` differ: syntactic measures are
    /// linkage-independent (a datatype node is one syntactic node regardless of its
    /// definition), so they close structurally right here at JIT. Runs on an explicit work
    /// stack: nothing recurs.
    ///
    /// Example: for datatypes `R` and `S`, the term `vector<R<S<u64>>>` gets `type_size = 4`,
    /// `type_depth = max(4)`, locals `value_depth = max(1)` / `layout_size = 1` (the vector),
    /// and the application chain `[S<u64>, R<r0>]` with `R`'s entry folded at depth 1.
    pub(crate) fn from_term(ty: &ArenaType, arena: &ArenaBuilder) -> PartialVMResult<Self> {
        // The syntactic measures, computed structurally over a worklist: each node is one
        // `type_size` node at its `type_depth` level; parameters contribute their own measures
        // (coefficients sum, offsets max on repeated parameters).
        let mut type_size = LinearForm::constant(0);
        let mut type_depth = MaxPlusForm::constant(0);
        let mut work: Vec<(&ArenaType, u64)> = vec![(ty, 0)];
        while let Some((ty, depth)) = work.pop() {
            match ty {
                ArenaType::TyParam(idx) => {
                    match type_size.terms.iter_mut().find(|t| t.param == *idx) {
                        Some(existing) => {
                            existing.coefficient = existing.coefficient.saturating_add(1)
                        }
                        None => type_size.terms.push(LinearTerm {
                            param: *idx,
                            coefficient: 1,
                        }),
                    }
                    match type_depth.terms.iter_mut().find(|t| t.param == *idx) {
                        Some(existing) => existing.offset = existing.offset.max(depth),
                        None => type_depth.terms.push(MaxPlusTerm {
                            param: *idx,
                            offset: depth,
                        }),
                    }
                }
                ArenaType::Vector(inner)
                | ArenaType::Reference(inner)
                | ArenaType::MutableReference(inner) => {
                    type_size.constant = type_size.constant.saturating_add(1);
                    type_depth.constant = type_depth.constant.max(depth.saturating_add(1));
                    work.push((inner, depth.saturating_add(1)));
                }
                ArenaType::DatatypeInstantiation(inst) => {
                    type_size.constant = type_size.constant.saturating_add(1);
                    type_depth.constant = type_depth.constant.max(depth.saturating_add(1));
                    let (_, args) = &**inst;
                    for arg in args.iter() {
                        work.push((arg, depth.saturating_add(1)));
                    }
                }
                // Primitives and uninstantiated datatypes: one node, one level.
                _ => {
                    type_size.constant = type_size.constant.saturating_add(1);
                    type_depth.constant = type_depth.constant.max(depth.saturating_add(1));
                }
            }
        }
        type_size.canonicalize();
        type_depth.canonicalize();

        // The term is a "field" of nothing: no wrapping datatype node, so the locals start at
        // zero and the fold sits at depth 0.
        let mut value_depth_local = MaxPlusForm::constant(0);
        let mut layout_size_local = LinearForm::constant(0);
        let mut linearizer = Linearizer::new();
        visit_field(
            ty,
            0,
            &mut value_depth_local,
            &mut layout_size_local,
            &mut linearizer,
        )?;
        value_depth_local.canonicalize();
        layout_size_local.canonicalize();
        let (keys, applications) = linearizer.finish(arena)?;
        Ok(ArenaTypeSizeFormula {
            type_size,
            type_depth,
            value_depth_local,
            layout_size_local,
            keys,
            applications,
        })
    }

    // The syntactic measures never depend on the linkage, so they solve straight off the
    // JIT-closed forms -- no dependency resolution, for the call sites (`realize_type`) that
    // need only these two.

    pub(crate) fn solve_type_size(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.type_size.solve(&project(args, |s| s.type_size))
    }

    pub(crate) fn solve_type_depth(&self, args: &[TypeSize]) -> PartialVMResult<u64> {
        self.type_depth.solve(&project(args, |s| s.type_depth))
    }

    /// The datatypes this formula's applications mention -- exactly the keys
    /// [`evaluate`](Self::evaluate)'s `key_formulae` must contain. The dispatch tables' work
    /// queue reads this list directly to resolve dependencies bottom-up (with cycle detection)
    /// without ever walking an [`ArenaType`].
    ///
    /// Example: `[T, S, R]` for `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }`.
    pub(crate) fn vtable_keys(&self) -> &[VirtualTableKey] {
        &self.keys
    }

    /// Close this formula against a linkage: given the resolved formula of every
    /// datatype in [`vtable_keys`](Self::vtable_keys), replay the linearized applications in
    /// one pass (one [`substitute`](PartialTypeSizeFormula::substitute) per entry, arguments
    /// read from earlier results), folding field roots into the through-field measures inline,
    /// and return the flat [`PartialTypeSizeFormula`] over the formula's own parameters.
    ///
    /// Example: `struct W<A> { x: u64, y: T<u64>, z: vector<R<S<A>>> }`, where `T`, `R`, and
    /// `S` are all shaped like `struct T<X> { t: X }` and so resolve to
    /// `value_depth = max(1, 1+x0)`. Starting from `value_depth_local = max(2)` (the `u64` and
    /// the vector):
    ///
    /// ```text
    /// r0 = T⟨u64⟩ = 2                              // field `y`: fold at depth 1
    /// r1 = S⟨x0⟩  = max(1, 1+x0)                   // subterm: no fold
    /// r2 = R⟨r1⟩  = max(2, 2+x0)                   // field `z`: fold at depth 2
    /// value_depth(W) = max(2, 1+r0, 2+r2) = max(4, 4+x0)
    /// ```
    ///
    /// The layout folds run the same pass but *add*: `layout_size(W) = 3 + r0 + r2 = 7 + x0`.
    /// (Depth folds on a shared parameter keep the deeper offset instead -- had `A` also been a
    /// direct field, its `1 + x0` would be absorbed by `z`'s `4 + x0`.)
    ///
    /// A missing key or out-of-range index is an invariant violation: the applications are
    /// JIT-compiled from well-formed types in dependency order, and the dispatch tables resolve
    /// every key in [`vtable_keys`](Self::vtable_keys) into `key_formulae` (which never evicts)
    /// before calling this. The cache is only ever probed by key (never iterated), so its
    /// hashing order is not a determinism hazard.
    pub(crate) fn evaluate(
        &self,
        key_formulae: &TypeCache,
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        fn broken(what: &str) -> PartialVMError {
            partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "{what} while evaluating a datatype size formula"
            )
        }

        // One borrow for the whole evaluation: no mutation happens here, so every lookup below
        // returns a reference into the cache -- no per-lookup clones.
        let key_formulae = key_formulae.read();
        let mut results: Vec<PartialTypeSizeFormula> = Vec::with_capacity(self.applications.len());
        let mut value_depth = self.value_depth_local.clone();
        let mut layout_size = self.layout_size_local.clone();
        for application in self.applications.iter() {
            let args = application
                .arguments
                .iter()
                .map(|argument| -> PartialVMResult<PartialTypeSizeFormula> {
                    let base = match argument.base {
                        ArgumentBase::Primitive => PartialTypeSizeFormula::primitive(),
                        ArgumentBase::TypeParameter(idx) => PartialTypeSizeFormula::parameter(idx),
                        ArgumentBase::Application(ndx) => results
                            .get(ndx as usize)
                            .cloned()
                            .ok_or_else(|| broken("out-of-order application reference"))?,
                    };
                    Ok((0..argument.vector_layers).fold(base, |formula, _| formula.wrap()))
                })
                .collect::<PartialVMResult<Vec<_>>>()?;
            let key = self
                .keys
                .get(application.datatype as usize)
                .ok_or_else(|| broken("out-of-bounds key index"))?;
            let formula = key_formulae
                .get(key)
                .ok_or_else(|| broken("unresolved datatype dependency"))?;
            let applied = formula.substitute(&args)?;
            if let Some(depth) = application.field_depth {
                value_depth.absorb(depth, &applied.value_depth);
                layout_size.absorb(1, &applied.layout_size);
            }
            results.push(applied);
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
