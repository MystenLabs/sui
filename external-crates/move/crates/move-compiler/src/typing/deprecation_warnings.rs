// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    expansion::ast::{self as E, ModuleIdent},
    ice,
    naming::ast as N,
    parser::ast::{ConstantName, DatatypeName, FunctionName},
    shared::{
        known_attributes::{AttributePosition, DeprecationAttribute, KnownAttribute},
        program_info::TypingProgramInfo,
        CompilationEnv, Identifier, Name,
    },
    typing::{ast as T, visitor::TypingVisitorContext},
};
use move_ir_types::location::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
    // #[deprecated]  -- if None
    // #[deprecated(note = b"message")] -- if Some(message)
    deprecation_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeprecationWarningId {
    // The named context in which this deprecation warning occured (function, struct, constant, etc.).
    call_site: ModuleMember<Name>,
    // The deprecation information for the member.
    deprecation: Box<Deprecation>,
}

type ModuleMember<T> = (ModuleIdent, T);
type DeprecationLocations = BTreeMap<AttributePosition, BTreeSet<Loc>>;

struct Deprecations<'env> {
    env: &'env mut CompilationEnv,
    program_info: Arc<TypingProgramInfo>,

    // Member deprecation information.
    deprecated_modules: BTreeMap<ModuleIdent, Deprecation>,
    deprecated_constants: BTreeMap<ModuleMember<ConstantName>, Deprecation>,
    deprecated_functions: BTreeMap<ModuleMember<FunctionName>, Deprecation>,
    deprecated_types: BTreeMap<ModuleMember<DatatypeName>, Deprecation>,

    // We store the locations of the deprecation warnings bucketed by "named context" (function,
    // struct, constant), to then merge them into a single warning at the end. This prevents us
    // from exploding with too many errors within a given named context.
    deprecation_warnings: BTreeMap<DeprecationWarningId, DeprecationLocations>,

    // Avoid registering multiple warnings to the same location.
    registered_warnings: BTreeSet<Loc>,

    // Information mutated during the visitor to set the current named context we are within in the
    // visitor.
    current_mident: Option<ModuleIdent>,
    current_named_context: Option<Name>,
}

// Entrypoint: Processes the `prog` and adds deprecation warnings to the `context`.
pub fn program(env: &mut CompilationEnv, prog: &mut T::Program) {
    let mut deprecations = Deprecations::new(env, prog);
    deprecations.visit(prog);
    deprecations.create_deprecation_warnings();
}

impl<'env> Deprecations<'env> {
    // Index the modules and build up the set of members that are deprecated for the program.
    fn new(env: &'env mut CompilationEnv, prog: &T::Program) -> Self {
        let mut s = Self {
            env,
            program_info: prog.info.clone(),
            registered_warnings: BTreeSet::new(),
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
        let position: AttributePosition =
            self.program_info.named_member_kind(*mident, name.0).into();
        self.deprecated_types
            .get(&(*mident, *name))
            .or_else(|| self.deprecated_modules.get(mident))
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
            .or_else(|| self.deprecated_modules.get(mident))
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
            .or_else(|| self.deprecated_modules.get(mident))
            .cloned()
            .map(|deprecation| (deprecation, AttributePosition::Function))
    }

    // Register a deprecation warning for a given deprecation and location. This collects all
    // `loc`s within a given named context. We will then then merge them into a single warning at
    // the end based on this bucketing.
    fn register_deprecation_warning(
        &mut self,
        (deprecation, member_type): (Deprecation, AttributePosition),
        loc: Loc,
    ) {
        // Don't register multiple warnings to the same location.
        if self.registered_warnings.contains(&loc) {
            return;
        }
        self.registered_warnings.insert(loc);
        let mident = self
            .current_mident
            .expect("ICE: current module should always be set when visiting deprecations");
        let name = self
            .current_named_context
            .expect("ICE: current named context should always be set when visiting deprecations");
        let deprecation_id = DeprecationWarningId {
            call_site: (mident, name),
            deprecation: Box::new(deprecation),
        };
        let entry = self.deprecation_warnings.entry(deprecation_id).or_default();
        let member_deprecations = entry.entry(member_type).or_default();
        member_deprecations.insert(loc);
    }

    // Process all the deprecation warnings and add them to the environment.
    // Note that we post-process warnings a bit here so that we don't emit warning for intra-module
    // calls for a deprecated module. Basically, if the module is deprecated and we are _not_
    // calling from within the deprecated module we report the deprecation warning. If we are
    // calling from within the deprecated module, we don't report a deprecation warning unless the
    // specific member being accessed is marked as deprecated. This prevents us for emitting
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
    fn create_deprecation_warnings(self) {
        for (DeprecationWarningId { deprecation, .. }, locs) in self
            .deprecation_warnings
            .into_iter()
            .filter(|(deprecation_id, _)| {
                // Dont' register a deprecation warning if the module is already deprecated and we are
                // calling from within the deprecated module.
                !(deprecation_id.deprecation.module_ident == deprecation_id.call_site.0
                    && deprecation_id.deprecation.location == AttributePosition::Module)
            })
        {
            let mut locs: Vec<_> = locs
                .into_iter()
                .flat_map(|(position, locs)| locs.into_iter().map(move |loc| (position, loc)))
                .collect();
            locs.sort_by_key(|(_, loc)| *loc);
            locs.reverse();

            let initial_diag = locs.pop().expect("ICE: locs should not be empty");

            let module_msg = if deprecation.location == AttributePosition::Module {
                ". It is deprecated since its whole module is marked deprecated"
            } else {
                ""
            };

            let location_string = match deprecation.location {
                AttributePosition::Module => {
                    format!("This {} is deprecated{}", initial_diag.0, module_msg,)
                }
                x => format!("This {} is deprecated", x),
            };

            let initial_message = match deprecation.deprecation_note {
                None => location_string,
                Some(note) => format!("{location_string}: {note}"),
            };

            let mut diag = diag!(
                TypeSafety::DeprecatedUsage,
                (initial_diag.1, initial_message)
            );

            for (position, loc) in locs {
                diag.add_secondary_label((loc, format!("Deprecated {position} also used here")));
            }
            self.env.add_diag(diag);
        }
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

    if deprecations.is_empty() {
        return None;
    }

    if deprecations.len() != 1 {
        env.add_diag(ice!((
            source_location,
            "ICE: verified that there is at at least one deprecation attribute above, \
            and expansion should have failed if there were multiple deprecation attributes."
        )));
        return None;
    }

    let (loc, _, attr) = deprecations
        .last()
        .expect("Verified deprecations is not empty above");

    let mut make_invalid_deprecation_diag = || {
        let mut diag = diag!(
            Attributes::InvalidUsage,
            (
                *loc,
                format!("Invalid '{}' attribute", DeprecationAttribute.name())
            )
        );
        let note = format!(
            "Deprecation attributes must be written as `#[{0}]` or `#[{0}(note = b\"message\")]`",
            DeprecationAttribute.name()
        );
        diag.add_note(note);
        env.add_diag(diag);
        None
    };

    match &attr.value {
        E::Attribute_::Name(_) => Some(Deprecation {
            source_location,
            location: attr_position,
            deprecation_note: None,
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
                        deprecation_note: Some(msg),
                        module_ident: mident,
                    })
                }
                _ => make_invalid_deprecation_diag(),
            }
        }
        E::Attribute_::Assigned(_, _) | E::Attribute_::Parameterized(_, _) => {
            make_invalid_deprecation_diag()
        }
    }
}

impl TypingVisitorContext for Deprecations<'_> {
    const VISIT_TYPES: bool = true;
    const VISIT_LVALUES: bool = true;

    // For each module Set the current module ident to `ident`.
    fn visit_module_custom(&mut self, ident: ModuleIdent, _mdef: &mut T::ModuleDefinition) -> bool {
        self.set_module(ident);
        false
    }

    // For each module member, set the current named context to the member's name
    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        struct_name: DatatypeName,
        _sdef: &mut N::StructDefinition,
    ) -> bool {
        self.set_named_context(struct_name.0);
        false
    }

    fn visit_enum_custom(
        &mut self,
        _module: ModuleIdent,
        enum_name: DatatypeName,
        _edef: &mut N::EnumDefinition,
    ) -> bool {
        self.set_named_context(enum_name.0);
        false
    }

    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        constant_name: ConstantName,
        _cdef: &mut T::Constant,
    ) -> bool {
        self.set_named_context(constant_name.0);
        false
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        function_name: FunctionName,
        _fdef: &mut T::Function,
    ) -> bool {
        self.set_named_context(function_name.0);
        false
    }

    fn visit_lvalue_custom(
        &mut self,
        _kind: &super::visitor::LValueKind,
        lvalue: &mut T::LValue,
    ) -> bool {
        match &lvalue.value {
            T::LValue_::Ignore => (),
            T::LValue_::Var { .. } => (),
            T::LValue_::UnpackVariant(mident, dname, _, _, _)
            | T::LValue_::BorrowUnpackVariant(_, mident, dname, _, _, _)
            | T::LValue_::BorrowUnpack(_, mident, dname, _, _)
            | T::LValue_::Unpack(mident, dname, _, _) => {
                if let Some(deprecation) = self.deprecated_type(mident, dname) {
                    self.register_deprecation_warning(deprecation.clone(), dname.loc());
                }
            }
        }
        false
    }

    fn visit_type_custom(&mut self, ty: &mut N::Type) -> bool {
        if let Some((mident, name)) = ty.value.type_name().and_then(|t| t.value.datatype_name()) {
            if let Some(deprecation) = self.deprecated_type(&mident, &name) {
                self.register_deprecation_warning(deprecation.clone(), name.loc());
            }
        }

        false
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
            }
            TUE::VariantMatch(e, data_access, _) => {
                if let Some(deprecation) = self.deprecated_type(&data_access.0, &data_access.1) {
                    self.register_deprecation_warning(deprecation.clone(), e.exp.loc);
                }
            }
            // Note: don't recurse into fields as that will be picked up by the visitor.
            TUE::Pack(mident, dname, _, _) | TUE::PackVariant(mident, dname, _, _, _) => {
                if let Some(deprecation) = self.deprecated_type(mident, dname) {
                    self.register_deprecation_warning(deprecation.clone(), dname.loc());
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

            TUE::BinopExp(_, _, _, _)
            | TUE::Vector(_, _, _, _)
            | TUE::Annotate(_, _)
            | TUE::Cast(_, _) => (),
            TUE::Assign(_, _, _) => (),
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
