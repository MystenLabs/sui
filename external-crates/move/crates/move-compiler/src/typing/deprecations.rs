// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::codes::TypeSafety,
    expansion::ast::{self as E, ModuleIdent},
    naming::ast::{self as N, StructFields, Type, Type_},
    parser::ast::{ConstantName, DatatypeName, FunctionName},
    shared::{
        known_attributes::{AttributePosition, KnownAttribute},
        CompilationEnv, Identifier, Name,
    },
    typing::{ast as T, visitor::TypingVisitorContext},
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::collections::{BTreeMap, BTreeSet};

const NOTE_STR: &str = "note";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Deprecation {
    // The source location of the deprecation attribute
    source_location: Loc,
    // The type of the member that is deprecated (function, constant, etc.)
    location: AttributePosition,
    // The module that the deprecated member belongs to. This is used in part to make sure we don't
    // register deprecation warnings for members within a given deprecated module calling within
    // that module.
    module_ident: ModuleIdent,
    // Information about the deprecation information depending on the deprecation attribute.
    deprecation_info: DeprecationInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DeprecationInfo {
    // #[deprecated]
    Deprecated,
    // #[deprecated(note = b"message")]
    DeprecatedWithNote(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeprecationId {
    mident: ModuleIdent,
    name: Name,
    deprecation: Deprecation,
}

type ModuleMemberRef<T> = (ModuleIdent, T);
type DeprecationLocations = BTreeMap<AttributePosition, BTreeSet<Loc>>;

struct Deprecations<'env> {
    env: &'env mut CompilationEnv,
    member_index: BTreeMap<ModuleMemberRef<Name>, AttributePosition>,

    deprecated_modules: BTreeMap<ModuleIdent, Deprecation>,
    deprecated_constants: BTreeMap<ModuleMemberRef<ConstantName>, Deprecation>,
    deprecated_functions: BTreeMap<ModuleMemberRef<FunctionName>, Deprecation>,
    deprecated_types: BTreeMap<ModuleMemberRef<DatatypeName>, Deprecation>,
    // We store the locations of the deprecation warnings bucketed by "named context" (function,
    // struct, constant), to then merge them into a single warning at the end. This prevents us
    // from exploding with too many errors within a given named context.
    deprecation_warnings: BTreeMap<DeprecationId, DeprecationLocations>,

    // Information mutated during the visitor to set the current named context we are within in the
    // visitor.
    current_mident: Option<ModuleIdent>,
    current_named_context: Option<Name>,
}

impl<'env> Deprecations<'env> {
    // Index the modules and build up the set of members that are deprecated for the program.
    fn new(env: &'env mut CompilationEnv, prog: &T::Program) -> Self {
        let member_index = Self::index(prog);
        let mut s = Self {
            env,
            member_index,
            deprecated_modules: BTreeMap::new(),
            deprecated_constants: BTreeMap::new(),
            deprecated_functions: BTreeMap::new(),
            deprecated_types: BTreeMap::new(),
            deprecation_warnings: BTreeMap::new(),
            current_mident: None,
            current_named_context: None,
        };
        for (mident, module_def) in prog.modules.key_cloned_iter() {
            if let Some(deprecation) = deprecations(
                s.env,
                AttributePosition::Module,
                &module_def.attributes,
                mident.loc,
                mident,
            ) {
                s.deprecated_modules.insert(mident, deprecation);
            }

            for (name, constant) in module_def.constants.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    s.env,
                    AttributePosition::Constant,
                    &constant.attributes,
                    name.loc(),
                    mident,
                ) {
                    s.deprecated_constants.insert((mident, name), deprecation);
                }
            }

            for (name, function) in module_def.functions.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    s.env,
                    AttributePosition::Function,
                    &function.attributes,
                    name.loc(),
                    mident,
                ) {
                    s.deprecated_functions.insert((mident, name), deprecation);
                }
            }

            for (name, datatype) in module_def.structs.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    s.env,
                    AttributePosition::Struct,
                    &datatype.attributes,
                    name.loc(),
                    mident,
                ) {
                    s.deprecated_types.insert((mident, name), deprecation);
                }
            }

            for (name, datatype) in module_def.enums.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    s.env,
                    AttributePosition::Enum,
                    &datatype.attributes,
                    name.loc(),
                    mident,
                ) {
                    s.deprecated_types.insert((mident, name), deprecation);
                }
            }
        }

        s
    }

    // Build up a position index/position map for all the members in the program (including dependencies).
    fn index(prog: &T::Program) -> BTreeMap<ModuleMemberRef<Name>, AttributePosition> {
        let mut member_index = BTreeMap::new();
        for (mident, module_info) in prog.info.modules.key_cloned_iter() {
            for (name, _) in module_info.constants.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Constant);
            }
            for (name, _) in module_info.functions.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Function);
            }
            for (name, _) in module_info.structs.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Struct);
            }
            for (name, _) in module_info.enums.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Enum);
            }
        }

        for (mident, module_def) in prog.modules.key_cloned_iter() {
            for (name, _) in module_def.constants.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Constant);
            }

            for (name, _) in module_def.functions.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Function);
            }

            for (name, _) in module_def.structs.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Struct);
            }

            for (name, _) in module_def.enums.key_cloned_iter() {
                member_index.insert((mident, name.0), AttributePosition::Enum);
            }
        }

        member_index
    }

    fn set_module(&mut self, mident: ModuleIdent) {
        self.current_mident = Some(mident);
    }

    fn set_named_context(&mut self, named_context: Name) {
        self.current_named_context = Some(named_context);
    }

    // Look up the deprecation information for a given type, constant, or function. If none is
    // found, see if the module that the type is in is deprecated.
    fn deprecated_type(
        &self,
        mident: &ModuleIdent,
        name: &DatatypeName,
    ) -> Option<(Deprecation, AttributePosition)> {
        let position = *self
            .member_index
            .get(&(*mident, name.0))
            .expect("All members are indexed");
        self.deprecated_types
            .get(&(*mident, *name))
            .or_else(|| self.deprecated_module(mident))
            .cloned()
            .map(|deprecation| (deprecation, position))
    }

    fn deprecated_constant(
        &self,
        mident: &ModuleIdent,
        name: &ConstantName,
    ) -> Option<(Deprecation, AttributePosition)> {
        self.deprecated_constants
            .get(&(*mident, *name))
            .or_else(|| self.deprecated_module(mident))
            .cloned()
            .map(|deprecation| (deprecation, AttributePosition::Constant))
    }

    fn deprecated_function(
        &self,
        mident: &ModuleIdent,
        name: &FunctionName,
    ) -> Option<(Deprecation, AttributePosition)> {
        self.deprecated_functions
            .get(&(*mident, *name))
            .or_else(|| self.deprecated_module(mident))
            .cloned()
            .map(|deprecation| (deprecation, AttributePosition::Function))
    }

    // Look up the deprecation information for a given module. If the module is deprecated, and we
    // are _not_ calling from within the deprecated module, return the deprecation information. If
    // we are calling from within the deprecated module, return None. This prevents us for emitting
    // warnings for things like the call to `f` in this module.
    // ```text
    // #[deprecated]
    // module 0x42::m {
    //   fun f() { }
    //   fun g() { f(); } // call to f will _not_ emit a deprecated warning since this is within 0x42::m
    // }
    // ```
    // Note that we will always emit a deprecated warning for any calls to function that is
    // annotated as deprecated, regardless of whether the call is within the module or not:
    // ```text
    // module 0x42::m {
    //   #[deprecated]
    //   fun f() { }
    //   fun g() { f(); } // call to f will emit a deprecated warning
    // }
    // ```
    //
    fn deprecated_module(&self, mident: &ModuleIdent) -> Option<&Deprecation> {
        // Dont' register a deprecation warning if the module is already deprecated and we are
        // calling from within the deprecated module.
        if Some(*mident) == self.current_mident {
            return None;
        }

        self.deprecated_modules.get(mident)
    }

    // Register a deprecation warning for a given deprecation and location. This collects all
    // `loc`s within a given named context. We will then then merge them into a single warning at
    // the end based on this bucketing.
    fn register_deprecation_warning(
        &mut self,
        (deprecation, member_type): (Deprecation, AttributePosition),
        loc: Loc,
    ) {
        let mident = self
            .current_mident
            .expect("ICE: current module should always be set when visiting deprecations");
        let name = self
            .current_named_context
            .expect("ICE: current named context should always be set when visiting deprecations");
        let deprecation_id = DeprecationId {
            mident,
            name,
            deprecation,
        };
        let entry = self.deprecation_warnings.entry(deprecation_id).or_default();
        let member_deprecations = entry.entry(member_type).or_default();
        member_deprecations.insert(loc);
    }

    #[growing_stack]
    fn handle_lvalue(&mut self, lval: &T::LValue) {
        match &lval.value {
            T::LValue_::Ignore => (),
            T::LValue_::Var { ty, .. } => {
                for (mident, name, loc) in qualified_datatype_name_of_type(ty) {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }
            }
            T::LValue_::UnpackVariant(mident, dname, _, ty_params, fields)
            | T::LValue_::BorrowUnpackVariant(_, mident, dname, _, ty_params, fields)
            | T::LValue_::BorrowUnpack(_, mident, dname, ty_params, fields)
            | T::LValue_::Unpack(mident, dname, ty_params, fields) => {
                if let Some(deprecation) = self.deprecated_type(mident, dname) {
                    self.register_deprecation_warning(deprecation.clone(), dname.loc());
                }

                for (mident, name, loc) in
                    ty_params.iter().flat_map(qualified_datatype_name_of_type)
                {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }

                for (_, _, (_, (ty, lval))) in fields.iter() {
                    self.handle_lvalue(lval);
                    for (mident, name, loc) in qualified_datatype_name_of_type(ty) {
                        if let Some(deprecation) = self.deprecated_type(mident, name) {
                            self.register_deprecation_warning(deprecation.clone(), loc);
                        }
                    }
                }
            }
        }
    }

    // Process all the deprecation warnings and add them to the environment.
    fn process_and_add_deprecation_warnings(self) {
        let attr_deprecation = |location| match location {
            AttributePosition::Module => TypeSafety::DeprecatedModule,
            AttributePosition::Constant => TypeSafety::DeprecatedConstant,
            AttributePosition::Struct => TypeSafety::DeprecatedStruct,
            AttributePosition::Enum => TypeSafety::DeprecatedEnum,
            AttributePosition::Function => TypeSafety::DeprecatedFunction,
            AttributePosition::AddressBlock
            | AttributePosition::Use
            | AttributePosition::Friend
            | AttributePosition::Spec => unreachable!(
                "ICE: Deprecations are not allowed in '{}'s and should have already been checked",
                location
            ),
        };

        for (DeprecationId { deprecation, .. }, locs) in self.deprecation_warnings {
            let mut locs: Vec<_> = locs
                .into_iter()
                .flat_map(|(position, locs)| {
                    Self::merge_locations(locs.into_iter().collect())
                        .into_iter()
                        .map(move |loc| (position, loc))
                })
                .collect();
            locs.sort_by_key(|(_, loc)| *loc);
            locs.reverse();

            let initial_diag = locs.pop().expect("ICE: locs should not be empty");

            let attr_deprecation = attr_deprecation(deprecation.location);
            let location_string = match deprecation.location {
                AttributePosition::Module => format!(
                        "This {} is deprecated since the whole module that it is declared in is marked deprecated",
                        initial_diag.0
                    ),
                x => format!("This {} is deprecated", x),
            };

            let initial_message = match deprecation.deprecation_info {
                DeprecationInfo::Deprecated => location_string,
                DeprecationInfo::DeprecatedWithNote(note) => format!("{location_string}: {note}"),
            };

            let mut diag = diag!(attr_deprecation, (initial_diag.1, initial_message));

            for (position, loc) in locs {
                diag.add_secondary_label((loc, format!("Deprecated {position} used here")));
            }
            self.env.add_diag(diag);
        }
    }

    // Merge contiguous locations into single locations within `locs`.
    fn merge_locations(mut locs: Vec<Loc>) -> Vec<Loc> {
        locs.sort();
        locs.reverse();

        let mut merged = vec![];
        let mut current = locs.pop().expect("ICE: locs should not be empty");
        let mut start = current.start();
        let mut end = current.end();

        while let Some(next) = locs.pop() {
            if current.file_hash() != next.file_hash() || end < next.start() {
                merged.push(Loc::new(current.file_hash(), start, end));
                current = next;
                start = current.start();
                end = current.end();
            } else {
                end = std::cmp::max(end, next.end());
            }
        }
        merged.push(Loc::new(current.file_hash(), start, end));

        merged
    }
}

// Entrypoint: Processes the `prog` and adds deprecation warnings to the `context`.
pub fn add_deprecation_warnings(env: &mut CompilationEnv, prog: &mut T::Program) {
    let mut deprecations = Deprecations::new(env, prog);
    deprecations.visit(prog);
    deprecations.process_and_add_deprecation_warnings();
}

// If `ty` is a qualified datatype name, return the module ident, datatype name, and location.
// Otherwise, return None.
#[growing_stack]
fn qualified_datatype_name_of_type(ty: &Type) -> Vec<(&ModuleIdent, &DatatypeName, Loc)> {
    match &ty.value {
        Type_::Apply(_, sp!(_, N::TypeName_::ModuleType(mident, name)), ty_args) => {
            std::iter::once((mident, name, ty.loc))
                .chain(ty_args.iter().flat_map(qualified_datatype_name_of_type))
                .collect()
        }
        Type_::Fun(tys, ty) => qualified_datatype_name_of_type(ty)
            .into_iter()
            .chain(tys.iter().flat_map(qualified_datatype_name_of_type))
            .collect::<Vec<_>>(),
        Type_::Ref(_, ty) => qualified_datatype_name_of_type(ty),
        _ => vec![],
    }
}

// Process the deprecation attributes for a given member (module, constant, function, etc.) and
// return `Optiong<Deprecation>` if there is a #[deprecated] attribute. If there are invalid
// #[deprecated] attributes (malformed, or multiple on the member), add an error diagnostic to
// `env` and return None.
fn deprecations(
    env: &mut CompilationEnv,
    attr_position: AttributePosition,
    attrs: &E::Attributes,
    source_location: Loc,
    mident: ModuleIdent,
) -> Option<Deprecation> {
    let deprecations: Vec<_> = attrs
        .iter()
        .filter(|(_, v, _)| matches!(v, KnownAttribute::Deprecation(_)))
        .collect();
    if deprecations.len() > 1 {
        for (loc, _, _) in deprecations[..deprecations.len() - 1].iter() {
            env.add_diag(diag!(
                TypeSafety::MultipleDeprecations,
                (
                    *loc,
                    "Multiple deprecation attributes. Remove this attribute"
                )
            ));
        }
    }

    if deprecations.is_empty() {
        return None;
    }

    let (loc, _, attr) = deprecations
        .last()
        .expect("Verified deprecations is not empty above");

    match &attr.value {
        E::Attribute_::Name(_) => Some(Deprecation {
            source_location,
            location: attr_position,
            deprecation_info: DeprecationInfo::Deprecated,
            module_ident: mident,
        }),
        E::Attribute_::Parameterized(_, assigns) if assigns.len() == 1 => {
            let param = assigns.key_cloned_iter().next().unwrap().1;
            match param {
                sp!(_, E::Attribute_::Assigned(sp!(_, name), attr_val))
                    if name.as_str() == NOTE_STR
                        && matches!(
                            &attr_val.value,
                            E::AttributeValue_::Value(sp!(_, E::Value_::Bytearray(_)))
                        ) =>
                {
                    let E::AttributeValue_::Value(sp!(_, E::Value_::Bytearray(b))) =
                        &attr_val.value
                    else {
                        unreachable!()
                    };
                    let msg = std::str::from_utf8(b).unwrap().to_string();
                    Some(Deprecation {
                        source_location,
                        location: attr_position,
                        deprecation_info: DeprecationInfo::DeprecatedWithNote(msg),
                        module_ident: mident,
                    })
                }
                _ => {
                    let mut diag = diag!(
                        TypeSafety::InvalidDeprecation,
                        (*loc, "Invalid deprecation attribute")
                    );
                    diag.add_note(
                        "Deprecation attributes must be written as `#[deprecated]` or `#[deprecated(note = b\"message\")]`",
                    );
                    env.add_diag(diag);
                    None
                }
            }
        }
        E::Attribute_::Assigned(_, _) | E::Attribute_::Parameterized(_, _) => {
            let mut diag = diag!(
                TypeSafety::InvalidDeprecation,
                (*loc, "Invalid deprecation attribute")
            );
            diag.add_note(
                "Deprecation attributes must be written as `#[deprecated]` or `#[deprecated(note = b\"message\")]`",
            );
            env.add_diag(diag);
            None
        }
    }
}

impl TypingVisitorContext for Deprecations<'_> {
    // For each module:
    // * Set the current module ident to `ident`.
    // * For each struct and enum in the module check if the types of any of the fields are deprecated.
    fn visit_module_custom(&mut self, ident: ModuleIdent, mdef: &mut T::ModuleDefinition) -> bool {
        self.set_module(ident);
        for (sname, sdef) in mdef.structs.key_cloned_iter() {
            self.set_named_context(sname.0);
            if let StructFields::Defined(_, fields) = &sdef.fields {
                for (_, _, (_, ty)) in fields.iter() {
                    for (mident, name, loc) in qualified_datatype_name_of_type(ty) {
                        if let Some(deprecation) = self.deprecated_type(mident, name) {
                            self.register_deprecation_warning(deprecation.clone(), loc);
                        }
                    }
                }
            }
        }

        for (ename, edef) in mdef.enums.key_cloned_iter() {
            self.set_named_context(ename.0);
            for (mident, name, loc) in
                edef.variants
                    .key_cloned_iter()
                    .flat_map(|(_, vdef)| match &vdef.fields {
                        N::VariantFields::Defined(_, fields) => fields
                            .key_cloned_iter()
                            .flat_map(|(_, (_, ty))| qualified_datatype_name_of_type(ty))
                            .collect(),
                        N::VariantFields::Empty => vec![],
                    })
            {
                if let Some(deprecation) = self.deprecated_type(mident, name) {
                    self.register_deprecation_warning(deprecation.clone(), loc);
                }
            }
        }

        false
    }

    // Set the current named context to the constant name.
    // Note: Don't need to progress into the constant since we don't have any types to check since these
    // types must all be primitive (and non-deprecated).
    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        constant_name: ConstantName,
        _cdef: &mut T::Constant,
    ) -> bool {
        self.set_named_context(constant_name.0);
        false
    }

    // Set the current named context to the function name.
    //
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        self.set_named_context(function_name.0);
        for (_, _, ty) in fdef.signature.parameters.iter() {
            for (mident, name, loc) in qualified_datatype_name_of_type(ty) {
                if let Some(deprecation) = self.deprecated_type(mident, name) {
                    self.register_deprecation_warning(deprecation.clone(), loc);
                }
            }
        }

        for (mident, name, loc) in qualified_datatype_name_of_type(&fdef.signature.return_type) {
            if let Some(deprecation) = self.deprecated_type(mident, name) {
                self.register_deprecation_warning(deprecation.clone(), loc);
            }
        }

        false
    }

    fn visit_seq_item(&mut self, sp!(_, seq_item): &mut T::SequenceItem) {
        use T::SequenceItem_ as SI;
        match seq_item {
            SI::Seq(e) => self.visit_exp(e),
            SI::Declare(lvals) => {
                for lval in lvals.value.iter() {
                    self.handle_lvalue(lval);
                }
            }
            SI::Bind(lvals, tys, e) => {
                for (mident, name, loc) in tys
                    .iter()
                    .flatten()
                    .flat_map(qualified_datatype_name_of_type)
                {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }

                for lval in lvals.value.iter() {
                    self.handle_lvalue(lval);
                }

                self.visit_exp(e);
            }
        }
    }

    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as TUE;
        let exp_loc = exp.exp.loc;
        match &exp.exp.value {
            TUE::Constant(mident, name) => {
                if let Some(deprecation) = self.deprecated_constant(mident, name) {
                    self.register_deprecation_warning(deprecation.clone(), exp_loc);
                }
            }
            TUE::ModuleCall(mcall) => {
                if let Some(deprecation) = self.deprecated_function(&mcall.module, &mcall.name) {
                    // Method calls `mcall.name.loc` points to the declaration locaiton of the
                    // function it resolves to and not the location of the call. This is why we
                    // first look for the location of the method call name, and then fallback to
                    // the function name since that points to the call location for non-method
                    // calls.
                    let name_loc = mcall.method_name.unwrap_or(mcall.name.0).loc;
                    self.register_deprecation_warning(deprecation.clone(), name_loc);
                }

                for (mident, name, loc) in mcall
                    .type_arguments
                    .iter()
                    .flat_map(qualified_datatype_name_of_type)
                {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }
            }
            TUE::VariantMatch(e, data_access, _) => {
                if let Some(deprecation) = self.deprecated_type(&data_access.0, &data_access.1) {
                    self.register_deprecation_warning(deprecation.clone(), e.exp.loc);
                }
            }
            TUE::Assign(lvals, tys_opt, _) => {
                for (mident, name, loc) in tys_opt
                    .iter()
                    .flatten()
                    .flat_map(qualified_datatype_name_of_type)
                {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }

                for lval in lvals.value.iter() {
                    self.handle_lvalue(lval);
                }
            }
            // Note: don't recurse into fields as that will be picked up by the visitor.
            TUE::Pack(mident, dname, ty_params, _)
            | TUE::PackVariant(mident, dname, _, ty_params, _) => {
                if let Some(deprecation) = self.deprecated_type(mident, dname) {
                    self.register_deprecation_warning(deprecation.clone(), dname.loc());
                }

                for (mident, name, loc) in
                    ty_params.iter().flat_map(qualified_datatype_name_of_type)
                {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }
            }
            TUE::BinopExp(_, _, ty, _)
            | TUE::Vector(_, _, ty, _)
            | TUE::Annotate(_, ty)
            | TUE::Cast(_, ty) => {
                for (mident, name, loc) in qualified_datatype_name_of_type(ty) {
                    if let Some(deprecation) = self.deprecated_type(mident, name) {
                        self.register_deprecation_warning(deprecation.clone(), loc);
                    }
                }
            }
            TUE::ErrorConstant {
                error_constant: Some(const_name),
                ..
            } => {
                if let Some(deprecation) =
                    self.deprecated_constant(&self.current_mident.unwrap(), const_name)
                {
                    self.register_deprecation_warning(deprecation.clone(), exp_loc);
                }
            }

            TUE::Unit { .. }
            | TUE::Value(_)
            | TUE::Move { .. }
            | TUE::Copy { .. }
            | TUE::Use(_)
            | TUE::Builtin(_, _)
            | TUE::IfElse(_, _, _)
            | TUE::Match(_, _)
            | TUE::While(_, _, _)
            | TUE::Loop { .. }
            | TUE::NamedBlock(_, _)
            | TUE::Block(_)
            | TUE::Mutate(_, _)
            | TUE::Return(_)
            | TUE::Abort(_)
            | TUE::Give(_, _)
            | TUE::Continue(_)
            | TUE::Dereference(_)
            | TUE::UnaryExp(_, _)
            | TUE::ExpList(_)
            | TUE::Borrow(_, _, _)
            | TUE::TempBorrow(_, _)
            | TUE::BorrowLocal(_, _)
            | TUE::ErrorConstant { .. }
            | TUE::UnresolvedError => (),
        }

        false
    }

    fn add_warning_filter_scope(&mut self, filter: crate::diagnostics::WarningFilters) {
        self.env.add_warning_filter_scope(filter);
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope();
    }
}
