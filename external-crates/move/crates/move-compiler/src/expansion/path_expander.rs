// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Name access chain (path) resolution. This is driven by the trait PathExpander, which works over
/// a DefnContext and resolves according to the rules of the selected expander.
use crate::{
    diagnostics::Diagnostic,
    editions::{Edition, FeatureGate, create_feature_error},
    expansion::{
        alias_map_builder::{
            AliasEntry, AliasMapBuilder, LeadingAccessEntry, MemberEntry, NameSpace,
            UnnecessaryAlias,
        },
        aliases::{AliasMap, AliasSet},
        ast::{self as E, Address, ModuleIdent, ModuleIdent_},
        legacy_aliases,
        name_validation::{ModuleMemberKind, is_valid_datatype_or_constant_name},
        translate::{
            DefnContext, ValueError, make_address, module_ident, top_level_address,
            top_level_address_opt, value_result,
        },
    },
    ice, ice_assert,
    parser::{
        ast::{self as P, ModuleName, NameAccess, NamePath, PathEntry, Type},
        syntax::make_loc,
    },
    shared::{
        ide::{AliasAutocompleteInfo, IDEAnnotation},
        known_attributes::{ExternalAttributeValue, ExternalAttributeValue_},
        *,
    },
};

use move_ir_types::location::{Loc, Spanned, sp};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

pub struct AccessPath {
    pub access: E::ModuleAccess,
    pub ptys_opt: Option<Spanned<Vec<P::Type>>>,
    pub is_macro: Option<Loc>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Type,
    ApplyNamed,
    ApplyPositional,
    Term,
    Pattern,
    Module, // Just used for errors
}

// -----------------------------------------------
// Errors
// -----------------------------------------------

// ----------------------
// Primary Errors

pub struct UnexpectedAccessError {
    result: AccessChainNameResult,
    access: Access,
    note: Option<&'static str>,
}

pub struct UnboundModuleError {
    module: Name,
}

pub struct AccessChainResolutionError {
    result: AccessChainNameResult,
}

pub struct IncompleteAccessError {
    loc: Loc,
}

pub struct AmbiguousAccessError {
    loc: Loc,
    kind: &'static str,
    possibles: (&'static str, &'static str),
}

pub struct UnexpectedAddressOrModule {
    loc: Loc,
    name_loc: Loc,
    access: Access,
}

pub struct LegacyGlobalAddressError {
    loc: Loc,
    edition: Edition,
}

pub struct LegacyInvalidName {
    loc: Loc,
    msg: &'static str,
    note: Option<&'static str>,
}

pub struct LegacyInvalidModule {
    loc: Loc,
    msg: &'static str,
}

// ----------------------
// Secondary Errors

pub struct InvalidTypeParameterError {
    loc: Loc,
    kind: &'static str,
    /// Since this is built from a name access chain, this must be a realized string. Otherwise we
    /// would need to copy the name access chain to compute it later.
    note: Option<String>,
}

pub struct InvalidMacroError {
    loc: Loc,
    kind: &'static str,
}

pub struct NeedsGlobalQualificationError {
    loc: Loc,
}

// ----------------------
// Expansion Error Enum

pub enum PathExpansionError {
    // Primary Errors
    UnexpectedAccessError(UnexpectedAccessError),
    UnboundModule(UnboundModuleError),
    AccessChainResolutionError(AccessChainResolutionError),
    IncompleteAccessError(IncompleteAccessError),
    AmbiguousAccessError(AmbiguousAccessError),
    UnexpectedAddressOrModule(UnexpectedAddressOrModule),
    ValueError(ValueError),
    LegacyGlobalAddressError(LegacyGlobalAddressError),
    LegacyInvalidName(LegacyInvalidName),
    LegacyInvalidModule(LegacyInvalidModule),
    // Secondary Errors
    InvalidTypeParameterError(InvalidTypeParameterError),
    InvalidMacroError(InvalidMacroError),
    NeedsGlobalQualificationError(NeedsGlobalQualificationError),
}

// -----------------------------------------------
// Path Expander
// -----------------------------------------------

pub struct PathExpanderResult<T> {
    pub result: Option<T>,
    pub errors: Vec<PathExpansionError>,
}

/// This trait describes the commands available to handle alias scopes and expanding name access
/// chains. This is used to model both legacy and modern path expansion.
pub trait PathExpander {
    // Push a new innermost alias scope
    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>>;

    // Push a number of type parameters onto the alias information in the path expander. They are
    // never resolved, but are tracked to apply appropriate shadowing.
    fn push_type_parameters(&mut self, tparams: Vec<&Name>);

    // Push a number of type parameters onto the alias information in the path expander. They are
    // never resolved, but are tracked to apply appropriate shadowing and suggestions.
    // NB: The namespace here should _not_ overlap with the existing callable namespace, but that
    // is only because we require `$` prefixes for lambda parameters. This could also be useful in
    // those cases.
    fn push_lambda_parameters(&mut self, lparams: Vec<&Name>);

    // Pop the innermost alias scope
    fn pop_alias_scope(&mut self) -> AliasSet;

    /// Returns an attribute value from a name access chain, plus possibly some error.
    /// These errors may be separate from the access chain resolution errors, allowing the
    /// compiler to continue processing the access chain and report later errors.
    fn name_access_chain_to_attribute_value(
        &mut self,
        context: &mut DefnContext,
        attribute_value: P::AttributeValue,
    ) -> PathExpanderResult<ExternalAttributeValue>;

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        name_chain: P::NameAccessChain,
    ) -> PathExpanderResult<AccessPath>;

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        name_chain: P::NameAccessChain,
    ) -> PathExpanderResult<E::ModuleIdent>;

    fn ide_autocomplete_suggestion(&mut self, context: &mut DefnContext, loc: Loc);
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl<T> PathExpanderResult<T> {
    pub fn err(errors: Vec<PathExpansionError>) -> Self {
        PathExpanderResult {
            result: None,
            errors,
        }
    }
}

pub fn make_access_path(
    access: E::ModuleAccess,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> AccessPath {
    AccessPath {
        access,
        ptys_opt,
        is_macro,
    }
}

macro_rules! path_entry {
    ($name:pat, $tyargs:pat, $is_macro:pat) => {
        PathEntry {
            name: $name,
            tyargs: $tyargs,
            is_macro: $is_macro,
        }
    };
}

macro_rules! single_entry {
    ($name:pat, $tyargs:pat, $is_macro:pat) => {
        P::NameAccessChain_::Single(path_entry!($name, $tyargs, $is_macro))
    };
}

macro_rules! access {
    ($access:pat, $ptys_opt:pat, $is_macro:pat) => {
        AccessPath {
            access: $access,
            ptys_opt: $ptys_opt,
            is_macro: $is_macro,
        }
    };
}

pub(crate) use access;
use move_symbol_pool::Symbol;

// -----------------------------------------------
// Error Impls
// -----------------------------------------------

// ----------------------
// Primary Errors

impl UnexpectedAccessError {
    fn into_diagnostic(self) -> Diagnostic {
        let UnexpectedAccessError {
            result,
            access,
            note,
        } = self;
        let loc = result.loc();
        let result = result.name();
        let case = match access {
            Access::Type | Access::ApplyNamed => "type",
            Access::ApplyPositional => "expression",
            Access::Term => "expression",
            Access::Pattern => "pattern constructor",
            Access::Module => "module",
        };
        let unexpected_msg = if result.starts_with('a') | result.starts_with('e') {
            format!(
                "Unexpected {0} identifier. An {0} identifier is not a valid {1}",
                result, case
            )
        } else {
            format!(
                "Unexpected {0} identifier. A {0} identifier is not a valid {1}",
                result, case
            )
        };
        let mut diag = crate::diag!(NameResolution::NamePositionMismatch, (loc, unexpected_msg));
        if let Some(note) = note {
            diag.add_note(note)
        };
        diag
    }
}

impl UnboundModuleError {
    fn into_diagnostic(self) -> Diagnostic {
        let UnboundModuleError { module } = self;
        crate::diag!(
            NameResolution::UnboundModule,
            (module.loc, format!("Unbound module alias '{}'", module))
        )
    }
}

impl AccessChainResolutionError {
    fn into_diagnostic(self) -> Diagnostic {
        let AccessChainResolutionError { result } = self;
        if let AccessChainNameResult::ResolutionFailure(inner, reason) = result {
            let loc = inner.loc();
            let msg = match reason {
                AccessChainFailure::InvalidKind(kind) => format!(
                    "Expected {} in this position, not {}",
                    kind,
                    inner.err_name()
                ),
                AccessChainFailure::UnresolvedAlias(name) => {
                    format!("Could not resolve the name '{}'", name)
                }
                AccessChainFailure::Suggestion(name, _) => {
                    format!("Could not resolve the name '{}'", name)
                }
            };
            let mut diag = crate::diag!(NameResolution::NamePositionMismatch, (loc, msg));
            if let AccessChainFailure::Suggestion(_, suggestion) = reason {
                let suggestion_loc = if suggestion.loc.is_valid() {
                    Some(suggestion.loc)
                } else {
                    None
                };
                add_suggestion(&mut diag, loc, suggestion, suggestion_loc);
            }
            diag
        } else {
            ice!((
                result.loc(),
                "ICE compiler miscalled access chain resolution error handler"
            ))
        }
    }
}

impl AmbiguousAccessError {
    fn into_diagnostic(self) -> Diagnostic {
        let AmbiguousAccessError {
            loc,
            kind,
            possibles: (p0, p1),
        } = self;
        let msg = format!(
            "Ambiguous {} value. It can resolve to both {} and {}",
            kind, p0, p1
        );
        crate::diag!(Attributes::AmbiguousAttributeValue, (loc, msg))
    }
}

impl UnexpectedAddressOrModule {
    fn into_diagnostic(self) -> Diagnostic {
        let UnexpectedAddressOrModule {
            loc,
            name_loc,
            access,
        } = self;
        let case = match access {
            Access::Type | Access::ApplyNamed | Access::ApplyPositional => "type",
            Access::Term => "expression",
            Access::Pattern => "pattern constructor",
            Access::Module => {
                return ice!(
                    (
                        loc,
                        "ICE expected a module name and got one, but tried to report an error"
                    ),
                    (name_loc, "Name location")
                );
            }
        };
        let unexpected_msg = format!(
            "Unexpected module identifier. A module identifier is not a valid {}",
            case
        );
        crate::diag!(
            NameResolution::NamePositionMismatch,
            (loc, unexpected_msg),
            (name_loc, "Expected a module member name".to_owned()),
        )
    }
}

impl LegacyGlobalAddressError {
    fn into_diagnostic(self) -> Diagnostic {
        let LegacyGlobalAddressError { loc, edition } = self;
        let mut diag: Diagnostic = create_feature_error(edition, FeatureGate::Move2024Paths, loc);
        diag.add_secondary_label((
            loc,
            "Paths that start with `::` are not valid in legacy move.",
        ));
        diag
    }
}

impl LegacyInvalidName {
    fn into_diagnostic(self) -> Diagnostic {
        let LegacyInvalidName { loc, msg, note } = self;
        let mut diag = crate::diag!(Syntax::InvalidName, (loc, msg));
        if let Some(note) = note {
            diag.add_note(note);
        }
        diag
    }
}

impl LegacyInvalidModule {
    fn into_diagnostic(self) -> Diagnostic {
        let LegacyInvalidModule { loc, msg } = self;
        crate::diag!(NameResolution::NamePositionMismatch, (loc, msg))
    }
}

// ----------------------
// Secondary Errors

impl IncompleteAccessError {
    fn into_diagnostic(self) -> Diagnostic {
        let loc = self.loc;
        let msg = "Incomplete name in this position. Expected an identifier after '::'";
        crate::diag!(Syntax::InvalidName, (loc, msg))
    }
}

impl InvalidTypeParameterError {
    fn into_diagnostic(self) -> Diagnostic {
        let InvalidTypeParameterError { loc, kind, note } = self;
        let mut diag = crate::diag!(
            NameResolution::InvalidTypeParameter,
            (loc, format!("Cannot use type parameters on {kind}"))
        );
        if let Some(note) = note {
            diag.add_note(note);
        };
        diag
    }
}

impl InvalidMacroError {
    fn into_diagnostic(self) -> Diagnostic {
        let InvalidMacroError { loc, kind } = self;
        crate::diag!(
            NameResolution::InvalidTypeParameter,
            (loc, format!("Cannot use {kind} as a macro invocation"))
        )
    }
}

impl NeedsGlobalQualificationError {
    fn into_diagnostic(self) -> Diagnostic {
        let NeedsGlobalQualificationError { loc } = self;
        crate::diag!(
            Migration::NeedsGlobalQualification,
            (loc, "Must globally qualify name")
        )
    }
}

// ----------------------
// Expander Error Enum

impl PathExpansionError {
    pub fn into_diagnostic(self) -> Diagnostic {
        match self {
            // Primary Errors
            PathExpansionError::UnexpectedAccessError(err) => err.into_diagnostic(),
            PathExpansionError::UnboundModule(err) => err.into_diagnostic(),
            PathExpansionError::AccessChainResolutionError(err) => err.into_diagnostic(),
            PathExpansionError::IncompleteAccessError(err) => err.into_diagnostic(),
            PathExpansionError::AmbiguousAccessError(err) => err.into_diagnostic(),
            PathExpansionError::UnexpectedAddressOrModule(err) => err.into_diagnostic(),
            PathExpansionError::ValueError(err) => err.into_diagnostic(),
            PathExpansionError::LegacyGlobalAddressError(err) => err.into_diagnostic(),
            PathExpansionError::LegacyInvalidName(err) => err.into_diagnostic(),
            PathExpansionError::LegacyInvalidModule(err) => err.into_diagnostic(),
            // Secondary Errors
            PathExpansionError::InvalidTypeParameterError(err) => err.into_diagnostic(),
            PathExpansionError::InvalidMacroError(err) => err.into_diagnostic(),
            PathExpansionError::NeedsGlobalQualificationError(err) => err.into_diagnostic(),
        }
    }

    // Primary Errors

    fn unexpected_access(
        result: AccessChainNameResult,
        access: Access,
        note: Option<&'static str>,
    ) -> Self {
        PathExpansionError::UnexpectedAccessError(UnexpectedAccessError {
            result,
            access,
            note,
        })
    }

    fn unbound_module(module: Name) -> Self {
        PathExpansionError::UnboundModule(UnboundModuleError { module })
    }

    fn access_chain_resolution(result: AccessChainNameResult) -> Self {
        PathExpansionError::AccessChainResolutionError(AccessChainResolutionError { result })
    }

    fn incomplete_access(loc: Loc) -> Self {
        PathExpansionError::IncompleteAccessError(IncompleteAccessError { loc })
    }

    fn ambiguous_access(
        loc: Loc,
        kind: &'static str,
        possibles: (&'static str, &'static str),
    ) -> Self {
        PathExpansionError::AmbiguousAccessError(AmbiguousAccessError {
            loc,
            kind,
            possibles,
        })
    }

    fn unexpected_address_or_module(loc: Loc, name_loc: Loc, access: Access) -> Self {
        PathExpansionError::UnexpectedAddressOrModule(UnexpectedAddressOrModule {
            loc,
            name_loc,
            access,
        })
    }

    fn legacy_global_address(loc: Loc, edition: Edition) -> Self {
        PathExpansionError::LegacyGlobalAddressError(LegacyGlobalAddressError { loc, edition })
    }

    fn legacy_invalid_name(loc: Loc, msg: &'static str, note: Option<&'static str>) -> Self {
        PathExpansionError::LegacyInvalidName(LegacyInvalidName { loc, msg, note })
    }

    fn legacy_invalid_module(loc: Loc, msg: &'static str) -> Self {
        PathExpansionError::LegacyInvalidModule(LegacyInvalidModule { loc, msg })
    }

    // Secondary Errors

    fn invalid_type_parameter(loc: Loc, kind: &'static str, note: Option<String>) -> Self {
        PathExpansionError::InvalidTypeParameterError(InvalidTypeParameterError { loc, kind, note })
    }

    fn invalid_macro(loc: Loc, kind: &'static str) -> Self {
        PathExpansionError::InvalidMacroError(InvalidMacroError { loc, kind })
    }

    fn needs_global_qualification(loc: Loc) -> Self {
        PathExpansionError::NeedsGlobalQualificationError(NeedsGlobalQualificationError { loc })
    }
}

//**************************************************************************************************
// Move 2024 Path Expander
//**************************************************************************************************

pub struct Move2024PathExpander {
    aliases: AliasMap,
}

#[derive(Debug, PartialEq, Eq)]
enum AccessChainNameResult {
    ModuleAccess(Loc, ModuleIdent, Name),
    Variant(Loc, Spanned<(ModuleIdent, Name)>, Name),
    Address(Loc, E::Address),
    ModuleIdent(Loc, E::ModuleIdent),
    UnresolvedName(Loc, Name),
    ResolutionFailure(Box<AccessChainNameResult>, AccessChainFailure),
    IncompleteChain(Loc),
}

struct AccessChainResult {
    result: AccessChainNameResult,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
    errors: Vec<PathExpansionError>,
}

#[derive(Debug, PartialEq, Eq)]
enum AccessChainFailure {
    UnresolvedAlias(Name),
    Suggestion(/* original */ Name, /* suggestion */ Name),
    InvalidKind(&'static str),
}

macro_rules! chain_result {
    ($result:pat, $ptys_opt:pat, $is_macro:pat, $errs:pat) => {
        AccessChainResult {
            result: $result,
            ptys_opt: $ptys_opt,
            is_macro: $is_macro,
            errors: $errs,
        }
    };
}

const MODULE_MEMBER_ACCESS: &str = "a type, function, or constant";
const LEADING_ACCESS: &str = "an address or module";
const ADDRESS_ACCESS: &str = "an address";
const ALL_ACCESS: &str = "a module, module member, or address";

fn make_leading_access_filter_fn(
    chain_length: usize,
    access: &Access,
) -> impl Fn(&Symbol, &LeadingAccessEntry) -> bool + '_ {
    fn filter_leading_access(
        chain_length: usize,
        access: &Access,
        entry: &LeadingAccessEntry,
    ) -> bool {
        use LeadingAccessEntry as LA;
        if chain_length == 0 {
            // This should be unreachable.
            return false;
        }
        // Depending on the chain length and access type, we filter suggestions based on the
        // entry we found.
        match (chain_length, access, entry) {
            // TYPE PARAMETERS
            // - Never suggest type params
            (_, _, LA::TypeParam) => false,
            // MODULE ACCESSES
            // - If there is a valid module that would fit for a module, suggest that.
            // - If the moodule has a leading address, suggest that.
            // - Never suggest a member for a module access.
            (_, Access::Module, LA::Module(_)) => true,
            (n, Access::Module, LA::Address(_)) if n > 1 => true,
            (_, Access::Module, LA::Member(_, _)) => false,
            // OTHER ACCESSES
            // For any other accesses:
            // - Only suggest members on chain length 2, in case it was meant to be an enum.
            // - Only suggest addresses or modules on chain length 2 or longer.
            (2, _, LA::Member(_, _)) => true,
            (_, _, LA::Member(_, _)) => false,
            (0 | 1, _, LA::Address(_) | LA::Module(_)) => false,
            (_, _, LA::Module(_)) => true,
            (2, _, LA::Address(_)) => false,
            (_, _, LA::Address(_)) => true,
        }
    }

    move |name: &Symbol, kind: &LeadingAccessEntry| {
        name != &ModuleName::SELF_NAME.into() && filter_leading_access(chain_length, access, kind)
    }
}

fn make_module_member_filter_fn<'access>(
    name: &Symbol,
    access: &'access Access,
) -> impl Fn(&Symbol, &MemberEntry) -> bool + 'access {
    fn filter_module_member(access: &Access, entry: &MemberEntry) -> bool {
        use ModuleMemberKind as K;
        let kind = match entry {
            // Bail on bound parameters
            MemberEntry::TypeParam => return false,
            // Suggest lambda params only for call positions
            MemberEntry::LambdaParam => return matches!(access, Access::ApplyPositional),
            MemberEntry::Member(_, _, kind) => kind,
        };
        match (access, kind) {
            // Never suggest for a term, as it may be a local.
            (Access::Term, _) => false,
            (Access::Type, K::Constant | K::Function) => false,
            (Access::Type, K::Struct | K::Enum) => true,
            (Access::ApplyNamed, K::Constant | K::Function | K::Enum) => false,
            (Access::ApplyNamed, K::Struct) => true,
            (Access::ApplyPositional, K::Function | K::Struct) => true,
            (Access::ApplyPositional, K::Constant | K::Enum) => false,
            (Access::Pattern, K::Constant | K::Struct) => true,
            (Access::Pattern, K::Function | K::Enum) => false,
            // Never suggest a member for a module, though this case should be unreachable.
            (Access::Module, _) => false,
        }
    }

    let name_is_uppercase = name.as_str().starts_with(UPPERCASE_LETTERS);

    move |member_name: &Symbol, kind: &MemberEntry| {
        let is_uppercase = member_name.as_str().starts_with(UPPERCASE_LETTERS);
        if is_uppercase != name_is_uppercase {
            return false;
        }
        filter_module_member(access, kind)
    }
}

impl Move2024PathExpander {
    pub(super) fn new() -> Move2024PathExpander {
        Move2024PathExpander {
            aliases: AliasMap::new(),
        }
    }

    fn resolve_root(
        &mut self,
        context: &mut DefnContext,
        chain_length: usize,
        access: &Access,
        sp!(loc, name): P::LeadingNameAccess,
    ) -> AccessChainNameResult {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        use P::LeadingNameAccess_ as LN;

        match name {
            LN::AnonymousAddress(address) => NR::Address(loc, E::Address::anonymous(loc, address)),
            LN::GlobalAddress(name) => {
                if let Some(address) = context
                    .named_address_mapping
                    .clone()
                    .expect("ICE no named address mapping")
                    .get(&name.value)
                {
                    NR::Address(loc, make_address(context, name, name.loc, *address))
                } else {
                    NR::ResolutionFailure(
                        Box::new(NR::UnresolvedName(loc, name)),
                        NF::UnresolvedAlias(name),
                    )
                }
            }
            LN::Name(name) => {
                let result = self.resolve_name(context, NameSpace::LeadingAccess, name);
                let NR::UnresolvedName(_, _) = result else {
                    return result;
                };
                let Some(suggestion) = self.aliases.suggest_leading_access(
                    make_leading_access_filter_fn(chain_length, access),
                    &name,
                ) else {
                    return NR::ResolutionFailure(Box::new(result), NF::UnresolvedAlias(name));
                };
                NR::ResolutionFailure(Box::new(result), NF::Suggestion(name, suggestion))
            }
        }
    }

    fn resolve_single(
        &mut self,
        context: &mut DefnContext,
        namespace: NameSpace,
        access: &Access,
        name: Name,
    ) -> AccessChainNameResult {
        use AccessChainNameResult as NR;

        let result = self.resolve_name(context, namespace, name);
        let NR::UnresolvedName(_, _) = result else {
            return result;
        };

        // Return early in two cases:
        // (A) If it looks like a variable usage, name resolution will handle suggestions, so we do
        //     not suggest anything.
        // (B) If this looks like a function call and it is lambda-bound, name resolution will
        //     handle suggestions, so we do not suggest anything.
        if matches!(access, Access::Term)
            || (matches!(access, Access::ApplyPositional)
                && self.aliases.is_lambda_parameter(&name))
        {
            return result;
        }

        // We ignore macro arguments, since they produce really unusual and useless suggestions.
        // We also ignore `_` as a name, since it is a special case and takes different error paths.
        let name_str = name.value.as_str();
        if name_str == "_" || name_str.starts_with("$") {
            return result;
        }
        let suggestion_opt = match namespace {
            NameSpace::ModuleMembers => self
                .aliases
                .suggest_module_member(make_module_member_filter_fn(&name.value, access), &name),
            NameSpace::LeadingAccess => self
                .aliases
                .suggest_leading_access(make_leading_access_filter_fn(1, access), &name),
        };
        let Some(suggestion) = suggestion_opt else {
            return result;
        };
        NR::ResolutionFailure(
            Box::new(result),
            AccessChainFailure::Suggestion(name, suggestion),
        )
    }

    fn resolve_name(
        &mut self,
        context: &mut DefnContext,
        namespace: NameSpace,
        name: Name,
    ) -> AccessChainNameResult {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        self.ide_autocomplete_suggestion(context, name.loc);
        match self.aliases.resolve(namespace, &name) {
            Some(AliasEntry::Member(_, mident, sp!(_, mem))) => {
                // We are preserving the name's original location, rather than referring to where
                // the alias was defined. The name represents JUST the member name, though, so we do
                // not change location of the module as we don't have this information.
                // TODO maybe we should also keep the alias reference (or its location)?
                NR::ModuleAccess(name.loc, mident, sp(name.loc, mem))
            }
            Some(AliasEntry::Module(_, mident)) => {
                // We are preserving the name's original location, rather than referring to where
                // the alias was defined. The name represents JUST the module name, though, so we do
                // not change location of the address as we don't have this information.
                // TODO maybe we should also keep the alias reference (or its location)?
                let sp!(
                    _,
                    ModuleIdent_ {
                        address,
                        module: ModuleName(sp!(_, module))
                    }
                ) = mident;
                let module = ModuleName(sp(name.loc, module));
                NR::ModuleIdent(name.loc, sp(name.loc, E::ModuleIdent_ { address, module }))
            }
            Some(AliasEntry::Address(_, address)) => {
                NR::Address(name.loc, make_address(context, name, name.loc, address))
            }
            Some(entry @ AliasEntry::TypeParam(_)) | Some(entry @ AliasEntry::LambdaParam(_)) => {
                context.add_diag(ice!((
                    name.loc,
                    format!("ICE alias map misresolved name as {:?}", entry),
                )));
                NR::UnresolvedName(name.loc, name)
            }
            None => {
                if let Some(entry) = self.aliases.resolve_any_for_error(&name) {
                    let msg = match namespace {
                        NameSpace::ModuleMembers => MODULE_MEMBER_ACCESS,
                        // we exclude types from this message since it would have been caught in
                        // the other namespace
                        NameSpace::LeadingAccess => LEADING_ACCESS,
                    };
                    let result = match entry {
                        AliasEntry::Address(_, address) => {
                            NR::Address(name.loc, make_address(context, name, name.loc, address))
                        }
                        AliasEntry::Module(_, mident) => NR::ModuleIdent(name.loc, mident),
                        AliasEntry::Member(_, mident, mem) => {
                            NR::ModuleAccess(name.loc, mident, mem)
                        }
                        entry @ (AliasEntry::TypeParam(_) | AliasEntry::LambdaParam(_)) => {
                            context.add_diag(ice!((
                                name.loc,
                                format!("ICE alias map misresolved name as {:?}", entry),
                            )));
                            NR::UnresolvedName(name.loc, name)
                        }
                    };
                    NR::ResolutionFailure(Box::new(result), NF::InvalidKind(msg))
                } else {
                    NR::UnresolvedName(name.loc, name)
                }
            }
        }
    }

    fn resolve_name_access_chain(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        sp!(loc, chain): P::NameAccessChain,
    ) -> AccessChainResult {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        use P::NameAccessChain_ as PN;

        let mut errors = Vec::new();

        fn check_tyargs(
            errors: &mut Vec<PathExpansionError>,
            tyargs: &Option<Spanned<Vec<Type>>>,
            result: &NR,
        ) {
            if let NR::Address(_, _) | NR::ModuleIdent(_, _) | NR::Variant(_, _, _) = result
                && let Some(tyargs) = tyargs
            {
                let loc = tyargs.loc;
                let kind = result.err_name();
                let mut note = None;
                if let NR::Variant(_, sp!(_, (mident, name)), variant) = result {
                    let tys = tyargs
                        .value
                        .iter()
                        .map(|ty| format!("{}", ty.value))
                        .collect::<Vec<_>>()
                        .join(",");
                    note = Some(format!(
                        "Type arguments are used with the enum, as '{mident}::{name}<{tys}>::{variant}'"
                    ));
                }
                errors.push(PathExpansionError::invalid_type_parameter(loc, kind, note));
            }
        }

        fn check_is_macro(
            errors: &mut Vec<PathExpansionError>,
            is_macro: &Option<Loc>,
            result: &NR,
        ) {
            if let NR::Address(_, _) | NR::ModuleIdent(_, _) = result
                && let Some(loc) = is_macro
            {
                errors.push(PathExpansionError::invalid_macro(*loc, result.err_name()));
            }
        }

        match chain.clone() {
            PN::Single(path_entry!(name, ptys_opt, is_macro)) => {
                use crate::naming::ast::BuiltinFunction_;
                use crate::naming::ast::BuiltinTypeName_;
                let namespace = match access {
                    Access::Type
                    | Access::ApplyNamed
                    | Access::ApplyPositional
                    | Access::Term
                    | Access::Pattern => NameSpace::ModuleMembers,
                    Access::Module => NameSpace::LeadingAccess,
                };

                // This is a hack to let `use std::vector` play nicely with `vector`,
                // plus preserve things like `u64`, etc.
                let result = if !matches!(access, Access::Module)
                    && (BuiltinFunction_::all_names().contains(&name.value)
                        || BuiltinTypeName_::all_names().contains(&name.value))
                {
                    NR::UnresolvedName(name.loc, name)
                } else {
                    self.resolve_single(context, namespace, &access, name)
                };
                AccessChainResult {
                    result,
                    ptys_opt,
                    is_macro,
                    errors,
                }
            }
            PN::Path(path) => {
                let NamePath {
                    root,
                    entries,
                    is_incomplete: incomplete,
                } = path;
                let chain_length = entries.len() + 1;
                let mut result = match self.resolve_root(context, chain_length, &access, root.name)
                {
                    // In Move Legacy, we always treated three-place names as fully-qualified.
                    // For migration mode, if we could have gotten the correct result doing so,
                    // we emit a migration change to globally-qualify that path and remediate
                    // the error.
                    result @ NR::ModuleIdent(_, _)
                        if entries.len() == 2
                            && context.env.edition(context.current_package)
                                == Edition::E2024_MIGRATION
                            && root.is_macro.is_none()
                            && root.tyargs.is_none() =>
                    {
                        if let Some(address) = top_level_address_opt(context, root.name) {
                            errors.push(PathExpansionError::needs_global_qualification(
                                root.name.loc,
                            ));
                            NR::Address(root.name.loc, address)
                        } else {
                            NR::ResolutionFailure(Box::new(result), NF::InvalidKind(ADDRESS_ACCESS))
                        }
                    }
                    result => result,
                };
                let mut ptys_opt = root.tyargs;
                let mut is_macro = root.is_macro;

                for entry in entries {
                    check_tyargs(&mut errors, &ptys_opt, &result);
                    check_is_macro(&mut errors, &is_macro, &result);
                    // ModuleAccess(ModuleIdent, Name),
                    // Variant(Spanned<(ModuleIdent, Name)>, Name),
                    match result {
                        NR::Variant(_, _, _) => {
                            result = NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind(ALL_ACCESS),
                            );
                            break;
                        }
                        NR::ModuleAccess(mloc, mident, member)
                            if context
                                .env
                                .supports_feature(context.current_package, FeatureGate::Enums) =>
                        {
                            let loc = make_loc(
                                mloc.file_hash(),
                                mloc.start() as usize,
                                entry.name.loc.end() as usize,
                            );
                            result = NR::Variant(loc, sp(mloc, (mident, member)), entry.name);
                            // For a variant, we use the type args from the access. We check these
                            // are empty or error.
                            check_tyargs(&mut errors, &entry.tyargs, &result);
                            if ptys_opt.is_none() && entry.tyargs.is_some() {
                                // This is an error, but we can try to be helpful.
                                ptys_opt = entry.tyargs;
                            }
                            check_is_macro(&mut errors, &entry.is_macro, &result);
                        }
                        NR::ModuleAccess(_mloc, _mident, _member) => {
                            result = NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind(LEADING_ACCESS),
                            );
                            break;
                        }

                        NR::Address(aloc, address) => {
                            let loc = make_loc(
                                aloc.file_hash(),
                                aloc.start() as usize,
                                entry.name.loc.end() as usize,
                            );
                            result = NR::ModuleIdent(
                                loc,
                                sp(loc, ModuleIdent_::new(address, ModuleName(entry.name))),
                            );
                            ptys_opt = entry.tyargs;
                            is_macro = entry.is_macro;
                        }
                        NR::ModuleIdent(mloc, mident) => {
                            let loc = make_loc(
                                mloc.file_hash(),
                                mloc.start() as usize,
                                entry.name.loc.end() as usize,
                            );
                            result = NR::ModuleAccess(loc, mident, entry.name);
                            ptys_opt = entry.tyargs;
                            is_macro = entry.is_macro;
                        }
                        NR::UnresolvedName(_, _) => {
                            context.add_diag(ice!((loc, "ICE access chain expansion failed")));
                            break;
                        }
                        NR::ResolutionFailure(_, _) => break,
                        NR::IncompleteChain(_) => break,
                    }
                }

                if incomplete {
                    result = NR::IncompleteChain(loc);
                }
                AccessChainResult {
                    result,
                    ptys_opt,
                    is_macro,
                    errors,
                }
            }
        }
    }
}

impl PathExpander for Move2024PathExpander {
    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
        self.aliases.push_alias_scope(loc, new_scope)
    }

    fn push_type_parameters(&mut self, tparams: Vec<&Name>) {
        self.aliases.push_type_parameters(tparams)
    }

    fn push_lambda_parameters(&mut self, lparams: Vec<&Name>) {
        self.aliases.push_lambda_parameters(lparams)
    }

    fn pop_alias_scope(&mut self) -> AliasSet {
        self.aliases.pop_scope()
    }

    fn name_access_chain_to_attribute_value(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, avalue_): P::AttributeValue,
    ) -> PathExpanderResult<ExternalAttributeValue> {
        use AccessChainNameResult as NR;
        use ExternalAttributeValue_ as EV;
        use P::AttributeValue_ as PV;

        let mut errors = Vec::new();
        let value_ = match avalue_ {
            PV::Value(v) => match value_result(context, v) {
                Ok(value) => EV::Value(value),
                Err(errs) => {
                    errors.extend(errs.into_iter().map(PathExpansionError::ValueError));
                    return PathExpanderResult::err(errors);
                }
            },
            // A bit strange, but we try to resolve it as a term and a module, and report
            // an error if they both resolve (to different things)
            PV::ModuleAccess(access_chain) => {
                ice_assert!(
                    context.reporter,
                    access_chain.value.tyargs().is_none(),
                    loc,
                    "Found tyargs"
                );
                ice_assert!(
                    context.reporter,
                    access_chain.value.is_macro().is_none(),
                    loc,
                    "Found macro"
                );
                let chain_result!(term_result, term_tyargs, term_is_macro, new_errors) =
                    self.resolve_name_access_chain(context, Access::Term, access_chain.clone());
                errors.extend(new_errors);
                assert!(term_tyargs.is_none());
                assert!(term_is_macro.is_none());
                let chain_result!(module_result, module_tyargs, module_is_macro, new_errors) =
                    self.resolve_name_access_chain(context, Access::Module, access_chain);
                errors.extend(new_errors);
                assert!(module_tyargs.is_none());
                assert!(module_is_macro.is_none());
                let result = match (term_result, module_result) {
                    (t_res, m_res) if t_res == m_res => t_res,
                    (NR::ResolutionFailure(_, _) | NR::UnresolvedName(_, _), other)
                    | (other, NR::ResolutionFailure(_, _) | NR::UnresolvedName(_, _)) => other,
                    (t_res, m_res) => {
                        let err = PathExpansionError::ambiguous_access(
                            loc,
                            "attribute",
                            (t_res.err_name(), m_res.err_name()),
                        );
                        errors.push(err);
                        return PathExpanderResult::err(errors);
                    }
                };
                match result {
                    NR::ModuleIdent(_, mident) => {
                        if context.module_members.get(&mident).is_none() {
                            errors.push(PathExpansionError::unbound_module(mident.value.module.0));
                        }
                        EV::Module(mident)
                    }
                    NR::ModuleAccess(loc, mident, member) => {
                        let access = sp(loc, E::ModuleAccess_::ModuleAccess(mident, member));
                        EV::ModuleAccess(access)
                    }
                    NR::Variant(loc, member_path, variant) => {
                        let access = sp(loc, E::ModuleAccess_::Variant(member_path, variant));
                        EV::ModuleAccess(access)
                    }
                    NR::UnresolvedName(loc, name) => {
                        EV::ModuleAccess(sp(loc, E::ModuleAccess_::Name(name)))
                    }
                    NR::Address(_, a) => EV::Address(a),
                    result @ NR::ResolutionFailure(_, _) => {
                        errors.push(PathExpansionError::access_chain_resolution(result));
                        return PathExpanderResult::err(errors);
                    }
                    NR::IncompleteChain(loc) => {
                        errors.push(PathExpansionError::incomplete_access(loc));
                        return PathExpanderResult::err(errors);
                    }
                }
            }
        };
        let result = Some(sp(loc, value_));
        PathExpanderResult { result, errors }
    }

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        chain: P::NameAccessChain,
    ) -> PathExpanderResult<AccessPath> {
        use AccessChainNameResult as NR;
        use E::ModuleAccess_ as EN;
        use P::NameAccessChain_ as PN;

        let mut loc = chain.loc;
        let mut errors = Vec::new();

        let (module_access, tyargs, is_macro) = match access {
            Access::ApplyPositional | Access::ApplyNamed | Access::Type => {
                let chain_result!(resolved_name, tyargs, is_macro, new_errors) =
                    self.resolve_name_access_chain(context, access, chain.clone());
                errors.extend(new_errors);

                match resolved_name {
                    NR::UnresolvedName(_, name) => {
                        loc = name.loc;
                        (EN::Name(name), tyargs, is_macro)
                    }
                    NR::ModuleAccess(_loc, mident, member) => {
                        let access = E::ModuleAccess_::ModuleAccess(mident, member);
                        (access, tyargs, is_macro)
                    }
                    NR::Variant(_loc, sp!(_mloc, (_mident, _member)), _)
                        if access == Access::Type =>
                    {
                        errors.push(PathExpansionError::unexpected_access(
                            resolved_name,
                            access,
                            Some("Variants may not be used as types. Use the enum instead."),
                        ));
                        // We could try to use the member access to try to keep going.
                        return PathExpanderResult::err(errors);
                    }
                    NR::Variant(_loc, member_path, variant) => {
                        let access = E::ModuleAccess_::Variant(member_path, variant);
                        (access, tyargs, is_macro)
                    }
                    NR::Address(_, _) => {
                        errors.push(PathExpansionError::unexpected_access(
                            resolved_name,
                            access,
                            None,
                        ));
                        return PathExpanderResult::err(errors);
                    }
                    NR::ModuleIdent(_, sp!(_, ModuleIdent_ { .. })) => {
                        errors.push(PathExpansionError::unexpected_access(
                            resolved_name,
                            access,
                            None,
                        ));
                        return PathExpanderResult::err(errors);
                    }
                    result @ NR::ResolutionFailure(_, _) => {
                        errors.push(PathExpansionError::access_chain_resolution(result));
                        return PathExpanderResult::err(errors);
                    }
                    NR::IncompleteChain(loc) => {
                        errors.push(PathExpansionError::incomplete_access(loc));
                        return PathExpanderResult::err(errors);
                    }
                }
            }
            Access::Term | Access::Pattern => match chain.value {
                PN::Single(path_entry!(name, tyargs, is_macro))
                    if !is_valid_datatype_or_constant_name(&name.to_string()) =>
                {
                    self.ide_autocomplete_suggestion(context, loc);
                    (EN::Name(name), tyargs, is_macro)
                }
                _ => {
                    let chain_result!(resolved_name, tyargs, is_macro, new_errors) =
                        self.resolve_name_access_chain(context, access, chain);
                    errors.extend(new_errors);

                    match resolved_name {
                        NR::UnresolvedName(_, name) => (EN::Name(name), tyargs, is_macro),
                        NR::ModuleAccess(_loc, mident, member) => {
                            let access = E::ModuleAccess_::ModuleAccess(mident, member);
                            (access, tyargs, is_macro)
                        }
                        NR::Variant(_loc, member_path, variant) => {
                            let access = E::ModuleAccess_::Variant(member_path, variant);
                            (access, tyargs, is_macro)
                        }
                        NR::Address(_, _) | NR::ModuleIdent(_, _) => {
                            errors.push(PathExpansionError::unexpected_access(
                                resolved_name,
                                access,
                                None,
                            ));
                            return PathExpanderResult::err(errors);
                        }
                        result @ NR::ResolutionFailure(_, _) => {
                            errors.push(PathExpansionError::access_chain_resolution(result));
                            return PathExpanderResult::err(errors);
                        }
                        NR::IncompleteChain(loc) => {
                            errors.push(PathExpansionError::incomplete_access(loc));
                            return PathExpanderResult::err(errors);
                        }
                    }
                }
            },
            Access::Module => {
                context.add_diag(ice!((
                    loc,
                    "ICE module access should never resolve to a module member"
                )));
                return PathExpanderResult::err(errors);
            }
        };
        let result = Some(make_access_path(sp(loc, module_access), tyargs, is_macro));
        PathExpanderResult { result, errors }
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        chain: P::NameAccessChain,
    ) -> PathExpanderResult<E::ModuleIdent> {
        use AccessChainNameResult as NR;

        let mut errors = Vec::new();

        let chain_result!(resolved_name, tyargs, is_macro, new_errors) =
            self.resolve_name_access_chain(context, Access::Module, chain);

        errors.extend(new_errors);

        assert!(tyargs.is_none());
        assert!(is_macro.is_none());

        let name = match resolved_name {
            NR::ModuleIdent(_, mident) => mident,
            NR::UnresolvedName(_, name) => {
                errors.push(PathExpansionError::unbound_module(name));
                return PathExpanderResult::err(errors);
            }
            NR::Address(_, _) => {
                errors.push(PathExpansionError::unexpected_access(
                    resolved_name,
                    Access::Module,
                    None,
                ));
                return PathExpanderResult::err(errors);
            }
            NR::ModuleAccess(_, _, _) | NR::Variant(_, _, _) => {
                errors.push(PathExpansionError::unexpected_access(
                    resolved_name,
                    Access::Module,
                    None,
                ));
                return PathExpanderResult::err(errors);
            }
            result @ NR::ResolutionFailure(_, _) => {
                errors.push(PathExpansionError::access_chain_resolution(result));
                return PathExpanderResult::err(errors);
            }
            NR::IncompleteChain(loc) => {
                errors.push(PathExpansionError::incomplete_access(loc));
                return PathExpanderResult::err(errors);
            }
        };
        let result = Some(name);
        PathExpanderResult { result, errors }
    }

    fn ide_autocomplete_suggestion(&mut self, context: &mut DefnContext, loc: Loc) {
        if context.env.ide_mode() {
            let info = self.aliases.get_ide_alias_information();
            context.add_ide_annotation(loc, IDEAnnotation::PathAutocompleteInfo(Box::new(info)));
        }
    }
}

impl AccessChainNameResult {
    fn loc(&self) -> Loc {
        match self {
            AccessChainNameResult::ModuleAccess(loc, _, _) => *loc,
            AccessChainNameResult::Variant(loc, _, _) => *loc,
            AccessChainNameResult::Address(loc, _) => *loc,
            AccessChainNameResult::ModuleIdent(loc, _) => *loc,
            AccessChainNameResult::UnresolvedName(loc, _) => *loc,
            AccessChainNameResult::ResolutionFailure(inner, _) => inner.loc(),
            AccessChainNameResult::IncompleteChain(loc) => *loc,
        }
    }

    const fn name(&self) -> &'static str {
        match self {
            AccessChainNameResult::ModuleAccess(_, _, _) => "module member",
            AccessChainNameResult::Variant(_, _, _) => "enum variant",
            AccessChainNameResult::ModuleIdent(_, _) => "module",
            AccessChainNameResult::UnresolvedName(_, _) => "name",
            AccessChainNameResult::Address(_, _) => "address",
            AccessChainNameResult::ResolutionFailure(inner, _) => inner.err_name(),
            AccessChainNameResult::IncompleteChain(_) => "",
        }
    }

    const fn err_name(&self) -> &'static str {
        match self {
            AccessChainNameResult::ModuleAccess(_, _, _) => "a module member",
            AccessChainNameResult::Variant(_, _, _) => "an enum variant",
            AccessChainNameResult::ModuleIdent(_, _) => "a module",
            AccessChainNameResult::UnresolvedName(_, _) => "a name",
            AccessChainNameResult::Address(_, _) => "an address",
            AccessChainNameResult::ResolutionFailure(inner, _) => inner.err_name(),
            AccessChainNameResult::IncompleteChain(_) => "",
        }
    }
}

//**************************************************************************************************
// Legacy Path Expander
//**************************************************************************************************

pub struct LegacyPathExpander {
    aliases: legacy_aliases::AliasMap,
    old_alias_maps: Vec<legacy_aliases::OldAliasMap>,
}

impl LegacyPathExpander {
    pub fn new() -> LegacyPathExpander {
        LegacyPathExpander {
            aliases: legacy_aliases::AliasMap::new(),
            old_alias_maps: vec![],
        }
    }
}

impl PathExpander for LegacyPathExpander {
    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
        self.old_alias_maps
            .push(self.aliases.add_and_shadow_all(loc, new_scope)?);
        Ok(vec![])
    }

    fn push_type_parameters(&mut self, tparams: Vec<&Name>) {
        self.old_alias_maps
            .push(self.aliases.shadow_for_type_parameters(tparams));
    }

    // Do nothing here -- lambdas are not supported legacy Move
    fn push_lambda_parameters(&mut self, _lparams: Vec<&Name>) {
        // We have to push _something_ here to keep the stack balanced
        self.old_alias_maps
            .push(self.aliases.shadow_for_type_parameters(vec![]));
    }

    fn pop_alias_scope(&mut self) -> AliasSet {
        if let Some(outer_scope) = self.old_alias_maps.pop() {
            self.aliases.set_to_outer_scope(outer_scope)
        } else {
            AliasSet::new()
        }
    }

    fn name_access_chain_to_attribute_value(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, avalue_): P::AttributeValue,
    ) -> PathExpanderResult<ExternalAttributeValue> {
        use ExternalAttributeValue_ as EV;
        use P::{AttributeValue_ as PV, LeadingNameAccess_ as LN, NameAccessChain_ as PN};

        let mut errors = Vec::new();

        let value = match avalue_ {
            PV::Value(v) => match value_result(context, v) {
                Ok(value) => EV::Value(value),
                Err(errs) => {
                    errors.extend(errs.into_iter().map(PathExpansionError::ValueError));
                    return PathExpanderResult::err(errors);
                }
            },
            // bit wonky, but this is the only spot currently where modules and expressions
            // exist in the same namespace.
            // TODO: consider if we want to just force all of these checks into the well-known
            // attribute setup
            PV::ModuleAccess(sp!(ident_loc, single_entry!(name, tyargs, is_macro)))
                if self.aliases.module_alias_get(&name).is_some() =>
            {
                self.ide_autocomplete_suggestion(context, loc);
                ice_assert!(context.reporter, tyargs.is_none(), loc, "Found tyargs");
                ice_assert!(context.reporter, is_macro.is_none(), loc, "Found macro");
                let sp!(_, mident_) = self.aliases.module_alias_get(&name).unwrap();
                let mident = sp(ident_loc, mident_);
                if !context.module_members.contains_key(&mident) {
                    errors.push(PathExpansionError::unbound_module(mident.value.module.0));
                }
                EV::Module(mident)
            }
            PV::ModuleAccess(sp!(ident_loc, PN::Path(path))) => {
                ice_assert!(context.reporter, !path.has_tyargs(), loc, "Found tyargs");
                ice_assert!(
                    context.reporter,
                    path.is_macro().is_none(),
                    loc,
                    "Found macro"
                );
                match (&path.root.name, &path.entries[..]) {
                    (sp!(aloc, LN::AnonymousAddress(a)), [n]) => {
                        let addr = Address::anonymous(*aloc, *a);
                        let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n.name)));
                        if !context.module_members.contains_key(&mident) {
                            errors.push(PathExpansionError::unbound_module(mident.value.module.0));
                        }
                        EV::Module(mident)
                    }
                    (sp!(aloc, LN::GlobalAddress(n1) | LN::Name(n1)), [n2])
                        if context
                            .named_address_mapping
                            .as_ref()
                            .map(|m| m.contains_key(&n1.value))
                            .unwrap_or(false) =>
                    {
                        let addr = top_level_address(
                            context,
                            /* suggest_declaration */ false,
                            sp(*aloc, LN::Name(*n1)),
                        );
                        let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2.name)));

                        if context.module_members.get(&mident).is_none() {
                            errors.push(PathExpansionError::unbound_module(mident.value.module.0));
                        }
                        EV::Module(mident)
                    }
                    _ => {
                        let result = self.name_access_chain_to_module_access(
                            context,
                            Access::Type,
                            sp(ident_loc, PN::Path(path)),
                        );
                        let PathExpanderResult {
                            result,
                            errors: new_errors,
                        } = result;
                        errors.extend(new_errors);
                        let Some(access) = result else {
                            return PathExpanderResult::err(errors);
                        };
                        EV::ModuleAccess(access.access)
                    }
                }
            }
            PV::ModuleAccess(ma) => {
                let result = self.name_access_chain_to_module_access(context, Access::Type, ma);
                let PathExpanderResult {
                    result,
                    errors: new_errors,
                } = result;
                errors.extend(new_errors);
                let Some(access) = result else {
                    return PathExpanderResult::err(errors);
                };
                EV::ModuleAccess(access.access)
            }
        };
        let result = Some(sp(loc, value));
        PathExpanderResult { result, errors }
    }

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        sp!(loc, ptn_): P::NameAccessChain,
    ) -> PathExpanderResult<AccessPath> {
        use E::ModuleAccess_ as EN;
        use P::{LeadingNameAccess_ as LN, NameAccessChain_ as PN};

        let mut errors = Vec::new();

        let tn_: AccessPath = match (access, ptn_) {
            (Access::Pattern, _) => {
                context.add_diag(ice!((
                    loc,
                    "Attempted to expand a variant with the legacy path expander"
                )));
                return PathExpanderResult::err(errors);
            }
            (
                Access::ApplyPositional | Access::ApplyNamed | Access::Type,
                single_entry!(name, tyargs, is_macro),
            ) => {
                if access == Access::Type {
                    ice_assert!(context.reporter, is_macro.is_none(), loc, "Found macro");
                }
                self.ide_autocomplete_suggestion(context, loc);
                let access = match self.aliases.member_alias_get(&name) {
                    Some((mident, mem)) => EN::ModuleAccess(mident, mem),
                    None => EN::Name(name),
                };
                make_access_path(sp(name.loc, access), tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro))
                if is_valid_datatype_or_constant_name(name.value.as_str()) =>
            {
                self.ide_autocomplete_suggestion(context, loc);
                let access = match self.aliases.member_alias_get(&name) {
                    Some((mident, mem)) => EN::ModuleAccess(mident, mem),
                    None => EN::Name(name),
                };
                make_access_path(sp(name.loc, access), tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro)) => {
                self.ide_autocomplete_suggestion(context, loc);
                make_access_path(sp(name.loc, EN::Name(name)), tyargs, is_macro)
            }
            (Access::Module, single_entry!(_name, _tyargs, _is_macro)) => {
                context.add_diag(ice!((
                    loc,
                    "ICE path resolution produced an impossible path for a module"
                )));
                return PathExpanderResult::err(errors);
            }
            (_, PN::Path(mut path)) => {
                if access == Access::Type {
                    ice_assert!(
                        context.reporter,
                        path.is_macro().is_none(),
                        loc,
                        "Found macro"
                    );
                }
                match (&path.root.name, &path.entries[..]) {
                    // Error cases
                    (sp!(aloc, LN::AnonymousAddress(_)), [_]) => {
                        errors.push(PathExpansionError::unexpected_address_or_module(
                            loc, *aloc, access,
                        ));
                        return PathExpanderResult::err(errors);
                    }
                    (sp!(_aloc, LN::GlobalAddress(_)), [_]) => {
                        errors.push(PathExpansionError::legacy_global_address(
                            loc,
                            context.env.edition(context.current_package),
                        ));
                        return PathExpanderResult::err(errors);
                    }
                    // Others
                    (sp!(_, LN::Name(n1)), [n2]) => {
                        self.ide_autocomplete_suggestion(context, n1.loc);
                        if let Some(mident) = self.aliases.module_alias_get(n1) {
                            let n2_name = n2.name;
                            let (tyargs, is_macro) = if !path.has_tyargs_last() {
                                errors.push(PathExpansionError::legacy_invalid_name(
                                    path.tyargs_loc().unwrap_or(n2_name.loc),
                                    "Invalid type argument position",
                                    Some("Type arguments may only be used with module members"),
                                ));
                                (None, path.is_macro())
                            } else {
                                (path.take_tyargs(), path.is_macro())
                            };
                            make_access_path(
                                sp(loc, EN::ModuleAccess(mident, n2_name)),
                                tyargs,
                                is_macro.copied(),
                            )
                        } else {
                            errors.push(PathExpansionError::unbound_module(*n1));
                            return PathExpanderResult::err(errors);
                        }
                    }
                    (ln, [n2, n3]) => {
                        self.ide_autocomplete_suggestion(context, ln.loc);
                        let ident_loc = make_loc(
                            ln.loc.file_hash(),
                            ln.loc.start() as usize,
                            n2.name.loc.end() as usize,
                        );
                        let addr =
                            top_level_address(context, /* suggest_declaration */ false, *ln);
                        let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2.name)));
                        let access = EN::ModuleAccess(mident, n3.name);
                        let (tyargs, is_macro) = if !(path.has_tyargs_last()) {
                            errors.push(PathExpansionError::legacy_invalid_name(
                                path.tyargs_loc().unwrap_or(n3.name.loc),
                                "Invalid type argument position",
                                Some("Type arguments may only be used with module members"),
                            ));
                            (None, path.is_macro())
                        } else {
                            (path.take_tyargs(), path.is_macro())
                        };
                        make_access_path(sp(loc, access), tyargs, is_macro.copied())
                    }
                    (_ln, []) => {
                        let diag = ice!((loc, "Found a root path with no additional entries"));
                        context.add_diag(diag);
                        return PathExpanderResult::err(errors);
                    }
                    (ln, [_n1, _n2, ..]) => {
                        self.ide_autocomplete_suggestion(context, ln.loc);
                        errors.push(PathExpansionError::legacy_invalid_name(
                            loc,
                            "Too many name segments",
                            Some("Names may only have 0, 1, or 2 segments separated by '::'"),
                        ));
                        return PathExpanderResult::err(errors);
                    }
                }
            }
        };
        let result = Some(tn_);
        PathExpanderResult { result, errors }
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, pn_): P::NameAccessChain,
    ) -> PathExpanderResult<E::ModuleIdent> {
        use P::NameAccessChain_ as PN;

        let mut errors = Vec::new();

        match pn_ {
            PN::Single(single) => {
                ice_assert!(
                    context.reporter,
                    single.tyargs.is_none(),
                    loc,
                    "Found tyargs"
                );
                ice_assert!(
                    context.reporter,
                    single.is_macro.is_none(),
                    loc,
                    "Found macro"
                );
                match self.aliases.module_alias_get(&single.name) {
                    None => {
                        errors.push(PathExpansionError::unbound_module(single.name));
                        PathExpanderResult::err(errors)
                    }
                    Some(mident) => {
                        let result = Some(mident);
                        PathExpanderResult { result, errors }
                    }
                }
            }
            PN::Path(path) => {
                ice_assert!(context.reporter, !path.has_tyargs(), loc, "Found tyargs");
                ice_assert!(
                    context.reporter,
                    path.is_macro().is_none(),
                    loc,
                    "Found macro"
                );
                match (&path.root.name, &path.entries[..]) {
                    (ln, [n]) => {
                        let pmident_ = P::ModuleIdent_ {
                            address: *ln,
                            module: ModuleName(n.name),
                        };
                        let result = Some(module_ident(context, sp(loc, pmident_)));
                        PathExpanderResult { result, errors }
                    }
                    // Error cases
                    (_ln, []) => {
                        context.add_diag(ice!((loc, "Found path with no path entries")));
                        PathExpanderResult::err(errors)
                    }
                    (ln, [n, m, ..]) => {
                        let ident_loc = make_loc(
                            ln.loc.file_hash(),
                            ln.loc.start() as usize,
                            n.name.loc.end() as usize,
                        );
                        // Process the module ident just for errors
                        let pmident_ = P::ModuleIdent_ {
                            address: *ln,
                            module: ModuleName(n.name),
                        };
                        let _ = module_ident(context, sp(ident_loc, pmident_));
                        let (loc, msg) = if path.entries.len() < 3 {
                            (
                                m.name.loc,
                                "Unexpected module member access. Expected a module identifier only",
                            )
                        } else {
                            (loc, "Unexpected access. Expected a module identifier only")
                        };
                        errors.push(PathExpansionError::legacy_invalid_module(loc, msg));
                        PathExpanderResult::err(errors)
                    }
                }
            }
        }
    }

    fn ide_autocomplete_suggestion(&mut self, context: &mut DefnContext, loc: Loc) {
        if context.env.ide_mode() && matches!(context.target_kind, P::TargetKind::Source { .. }) {
            let mut info = AliasAutocompleteInfo::new();
            for (name, addr) in context.named_address_mapping.clone().unwrap().iter() {
                info.addresses.insert(*name, *addr);
            }
            for (_, name, (_, mident)) in self.aliases.modules.iter() {
                info.modules.insert(*name, *mident);
            }
            for (_, name, (_, (mident, member))) in self.aliases.members.iter() {
                info.members
                    .entry((*mident, member.value))
                    .or_default()
                    .insert(*name);
            }
            let annotation = IDEAnnotation::PathAutocompleteInfo(Box::new(info));
            context.add_ide_annotation(loc, annotation)
        }
    }
}
