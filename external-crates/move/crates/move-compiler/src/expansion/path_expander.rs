// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Name access chain (path) resolution. This is driven by the trait PathExpander, which works over
/// a DefnContext and resolves according to the rules of the selected expander.
use crate::{
    diag,
    diagnostics::Diagnostic,
    editions::{create_feature_error, Edition, FeatureGate},
    expansion::{
        alias_map_builder::{AliasEntry, AliasMapBuilder, NameSpace, UnnecessaryAlias},
        aliases::{AliasMap, AliasSet},
        ast::{self as E, Address, ModuleIdent, ModuleIdent_},
        legacy_aliases,
        name_validation::is_valid_datatype_or_constant_name,
        translate::{
            make_address, module_ident, top_level_address, top_level_address_opt, value,
            DefnContext,
        },
    },
    ice, ice_assert,
    parser::{
        ast::{self as P, ModuleName, NameAccess, NamePath, PathEntry, Type},
        syntax::make_loc,
    },
    shared::{
        ide::{AliasAutocompleteInfo, IDEAnnotation},
        *,
    },
};

use move_ir_types::location::{sp, Loc, Spanned};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

pub struct ModuleAccessResult {
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

    // Pop the innermost alias scope
    fn pop_alias_scope(&mut self) -> AliasSet;

    fn name_access_chain_to_attribute_value(
        &mut self,
        context: &mut DefnContext,
        attribute_value: P::AttributeValue,
    ) -> Option<E::AttributeValue>;

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        name_chain: P::NameAccessChain,
    ) -> Option<ModuleAccessResult>;

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        name_chain: P::NameAccessChain,
    ) -> Option<E::ModuleIdent>;

    fn ide_autocomplete_suggestion(&mut self, context: &mut DefnContext, loc: Loc);
}

pub fn make_access_result(
    access: E::ModuleAccess,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> ModuleAccessResult {
    ModuleAccessResult {
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

macro_rules! access_result {
    ($access:pat, $ptys_opt:pat, $is_macro:pat) => {
        ModuleAccessResult {
            access: $access,
            ptys_opt: $ptys_opt,
            is_macro: $is_macro,
        }
    };
}

pub(crate) use access_result;

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
}

#[derive(Debug, PartialEq, Eq)]
enum AccessChainFailure {
    UnresolvedAlias(Name),
    InvalidKind(String),
}

macro_rules! chain_result {
    ($result:pat, $ptys_opt:pat, $is_macro:pat) => {
        AccessChainResult {
            result: $result,
            ptys_opt: $ptys_opt,
            is_macro: $is_macro,
        }
    };
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
            LN::Name(name) => match self.resolve_name(context, NameSpace::LeadingAccess, name) {
                result @ NR::UnresolvedName(_, _) => {
                    NR::ResolutionFailure(Box::new(result), NF::UnresolvedAlias(name))
                }
                other => other,
            },
        }
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
            Some(AliasEntry::TypeParam(_)) => {
                context.add_diag(ice!((
                    name.loc,
                    "ICE alias map misresolved name as type param"
                )));
                NR::UnresolvedName(name.loc, name)
            }
            None => {
                if let Some(entry) = self.aliases.resolve_any_for_error(&name) {
                    let msg = match namespace {
                        NameSpace::ModuleMembers => "a type, function, or constant".to_string(),
                        // we exclude types from this message since it would have been caught in
                        // the other namespace
                        NameSpace::LeadingAccess => "an address or module".to_string(),
                    };
                    let result = match entry {
                        AliasEntry::Address(_, address) => {
                            NR::Address(name.loc, make_address(context, name, name.loc, address))
                        }
                        AliasEntry::Module(_, mident) => NR::ModuleIdent(name.loc, mident),
                        AliasEntry::Member(_, mident, mem) => {
                            NR::ModuleAccess(name.loc, mident, mem)
                        }
                        AliasEntry::TypeParam(_) => {
                            context.add_diag(ice!((
                                name.loc,
                                "ICE alias map misresolved name as type param"
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

        fn check_tyargs(
            context: &mut DefnContext,
            tyargs: &Option<Spanned<Vec<Type>>>,
            result: &NR,
        ) {
            if let NR::Address(_, _) | NR::ModuleIdent(_, _) | NR::Variant(_, _, _) = result {
                if let Some(tyargs) = tyargs {
                    let mut diag = diag!(
                        NameResolution::InvalidTypeParameter,
                        (
                            tyargs.loc,
                            format!("Cannot use type parameters on {}", result.err_name())
                        )
                    );
                    if let NR::Variant(_, sp!(_, (mident, name)), variant) = result {
                        let tys = tyargs
                            .value
                            .iter()
                            .map(|ty| format!("{}", ty.value))
                            .collect::<Vec<_>>()
                            .join(",");
                        diag.add_note(format!("Type arguments are used with the enum, as '{mident}::{name}<{tys}>::{variant}'"))
                    }
                    context.add_diag(diag);
                }
            }
        }

        fn check_is_macro(context: &mut DefnContext, is_macro: &Option<Loc>, result: &NR) {
            if let NR::Address(_, _) | NR::ModuleIdent(_, _) = result {
                if let Some(loc) = is_macro {
                    context.add_diag(diag!(
                        NameResolution::InvalidTypeParameter,
                        (
                            *loc,
                            format!("Cannot use {} as a macro invocation", result.err_name())
                        )
                    ));
                }
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
                    self.resolve_name(context, namespace, name)
                };
                AccessChainResult {
                    result,
                    ptys_opt,
                    is_macro,
                }
            }
            PN::Path(path) => {
                let NamePath {
                    root,
                    entries,
                    is_incomplete: incomplete,
                } = path;
                let mut result = match self.resolve_root(context, root.name) {
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
                            context.add_diag(diag!(
                                Migration::NeedsGlobalQualification,
                                (root.name.loc, "Must globally qualify name")
                            ));
                            NR::Address(root.name.loc, address)
                        } else {
                            NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind("an address".to_string()),
                            )
                        }
                    }
                    result => result,
                };
                let mut ptys_opt = root.tyargs;
                let mut is_macro = root.is_macro;

                for entry in entries {
                    check_tyargs(context, &ptys_opt, &result);
                    check_is_macro(context, &is_macro, &result);
                    // ModuleAccess(ModuleIdent, Name),
                    // Variant(Spanned<(ModuleIdent, Name)>, Name),
                    match result {
                        NR::Variant(_, _, _) => {
                            result = NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind("a module, module member, or address".to_string()),
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
                            check_tyargs(context, &entry.tyargs, &result);
                            if ptys_opt.is_none() && entry.tyargs.is_some() {
                                // This is an error, but we can try to be helpful.
                                ptys_opt = entry.tyargs;
                            }
                            check_is_macro(context, &entry.is_macro, &result);
                        }
                        NR::ModuleAccess(_mloc, _mident, _member) => {
                            result = NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind("a module or address".to_string()),
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

    fn pop_alias_scope(&mut self) -> AliasSet {
        self.aliases.pop_scope()
    }

    fn name_access_chain_to_attribute_value(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, avalue_): P::AttributeValue,
    ) -> Option<E::AttributeValue> {
        use AccessChainNameResult as NR;
        use E::AttributeValue_ as EV;
        use P::AttributeValue_ as PV;
        Some(sp(
            loc,
            match avalue_ {
                PV::Value(v) => EV::Value(value(context, v)?),
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
                    let chain_result!(term_result, term_tyargs, term_is_macro) =
                        self.resolve_name_access_chain(context, Access::Term, access_chain.clone());
                    assert!(term_tyargs.is_none());
                    assert!(term_is_macro.is_none());
                    let chain_result!(module_result, module_tyargs, module_is_macro) =
                        self.resolve_name_access_chain(context, Access::Module, access_chain);
                    assert!(module_tyargs.is_none());
                    assert!(module_is_macro.is_none());
                    let result = match (term_result, module_result) {
                        (t_res, m_res) if t_res == m_res => t_res,
                        (NR::ResolutionFailure(_, _) | NR::UnresolvedName(_, _), other)
                        | (other, NR::ResolutionFailure(_, _) | NR::UnresolvedName(_, _)) => other,
                        (t_res, m_res) => {
                            let msg = format!(
                                "Ambiguous attribute value. It can resolve to both {} and {}",
                                t_res.err_name(),
                                m_res.err_name()
                            );
                            context
                                .add_diag(diag!(Attributes::AmbiguousAttributeValue, (loc, msg)));
                            return None;
                        }
                    };
                    match result {
                        NR::ModuleIdent(_, mident) => {
                            if context.module_members.get(&mident).is_none() {
                                context.add_diag(diag!(
                                    NameResolution::UnboundModule,
                                    (loc, format!("Unbound module '{}'", mident))
                                ));
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
                            context.add_diag(access_chain_resolution_error(result));
                            return None;
                        }
                        NR::IncompleteChain(loc) => {
                            context.add_diag(access_chain_incomplete_error(loc));
                            return None;
                        }
                    }
                }
            },
        ))
    }

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        chain: P::NameAccessChain,
    ) -> Option<ModuleAccessResult> {
        use AccessChainNameResult as NR;
        use E::ModuleAccess_ as EN;
        use P::NameAccessChain_ as PN;

        let mut loc = chain.loc;

        let (module_access, tyargs, is_macro) = match access {
            Access::ApplyPositional | Access::ApplyNamed | Access::Type => {
                let chain_result!(resolved_name, tyargs, is_macro) =
                    self.resolve_name_access_chain(context, access, chain.clone());
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
                        let mut diag = unexpected_access_error(
                            resolved_name.loc(),
                            resolved_name.name(),
                            access,
                        );
                        diag.add_note("Variants may not be used as types. Use the enum instead.");
                        context.add_diag(diag);
                        // We could try to use the member access to try to keep going.
                        return None;
                    }
                    NR::Variant(_loc, member_path, variant) => {
                        let access = E::ModuleAccess_::Variant(member_path, variant);
                        (access, tyargs, is_macro)
                    }
                    NR::Address(_, _) => {
                        context.add_diag(unexpected_access_error(
                            resolved_name.loc(),
                            resolved_name.name(),
                            access,
                        ));
                        return None;
                    }
                    NR::ModuleIdent(_, sp!(_, ModuleIdent_ { address, module })) => {
                        let mut diag = unexpected_access_error(
                            resolved_name.loc(),
                            resolved_name.name(),
                            access,
                        );
                        let base_str = format!("{}", chain);
                        let realized_str = format!("{}::{}", address, module);
                        if base_str != realized_str {
                            diag.add_note(format!(
                                "Resolved '{}' to module identifier '{}'",
                                base_str, realized_str
                            ));
                        }
                        context.add_diag(diag);
                        return None;
                    }
                    result @ NR::ResolutionFailure(_, _) => {
                        context.add_diag(access_chain_resolution_error(result));
                        return None;
                    }
                    NR::IncompleteChain(loc) => {
                        context.add_diag(access_chain_incomplete_error(loc));
                        return None;
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
                    let chain_result!(resolved_name, tyargs, is_macro) =
                        self.resolve_name_access_chain(context, access, chain);
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
                            context.add_diag(unexpected_access_error(
                                resolved_name.loc(),
                                resolved_name.name(),
                                access,
                            ));
                            return None;
                        }
                        result @ NR::ResolutionFailure(_, _) => {
                            context.add_diag(access_chain_resolution_error(result));
                            return None;
                        }
                        NR::IncompleteChain(loc) => {
                            context.add_diag(access_chain_incomplete_error(loc));
                            return None;
                        }
                    }
                }
            },
            Access::Module => {
                context.add_diag(ice!((
                    loc,
                    "ICE module access should never resolve to a module member"
                )));
                return None;
            }
        };
        Some(make_access_result(sp(loc, module_access), tyargs, is_macro))
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        chain: P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        use AccessChainNameResult as NR;
        let chain_result!(resolved_name, tyargs, is_macro) =
            self.resolve_name_access_chain(context, Access::Module, chain);
        assert!(tyargs.is_none());
        assert!(is_macro.is_none());
        match resolved_name {
            NR::ModuleIdent(_, mident) => Some(mident),
            NR::UnresolvedName(_, name) => {
                context.add_diag(unbound_module_error(name));
                None
            }
            NR::Address(_, _) => {
                context.add_diag(unexpected_access_error(
                    resolved_name.loc(),
                    "address".to_string(),
                    Access::Module,
                ));
                None
            }
            NR::ModuleAccess(_, _, _) | NR::Variant(_, _, _) => {
                context.add_diag(unexpected_access_error(
                    resolved_name.loc(),
                    "module member".to_string(),
                    Access::Module,
                ));
                None
            }
            result @ NR::ResolutionFailure(_, _) => {
                context.add_diag(access_chain_resolution_error(result));
                None
            }
            NR::IncompleteChain(loc) => {
                context.add_diag(access_chain_incomplete_error(loc));
                None
            }
        }
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

    fn name(&self) -> String {
        match self {
            AccessChainNameResult::ModuleAccess(_, _, _) => "module member".to_string(),
            AccessChainNameResult::Variant(_, _, _) => "enum variant".to_string(),
            AccessChainNameResult::ModuleIdent(_, _) => "module".to_string(),
            AccessChainNameResult::UnresolvedName(_, _) => "name".to_string(),
            AccessChainNameResult::Address(_, _) => "address".to_string(),
            AccessChainNameResult::ResolutionFailure(inner, _) => inner.err_name(),
            AccessChainNameResult::IncompleteChain(_) => "".to_string(),
        }
    }

    fn err_name(&self) -> String {
        match self {
            AccessChainNameResult::ModuleAccess(_, _, _) => "a module member".to_string(),
            AccessChainNameResult::Variant(_, _, _) => "an enum variant".to_string(),
            AccessChainNameResult::ModuleIdent(_, _) => "a module".to_string(),
            AccessChainNameResult::UnresolvedName(_, _) => "a name".to_string(),
            AccessChainNameResult::Address(_, _) => "an address".to_string(),
            AccessChainNameResult::ResolutionFailure(inner, _) => inner.err_name(),
            AccessChainNameResult::IncompleteChain(_) => "".to_string(),
        }
    }
}

fn unexpected_access_error(loc: Loc, result: String, access: Access) -> Diagnostic {
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
    diag!(NameResolution::NamePositionMismatch, (loc, unexpected_msg),)
}

fn unbound_module_error(name: Name) -> Diagnostic {
    diag!(
        NameResolution::UnboundModule,
        (name.loc, format!("Unbound module alias '{}'", name))
    )
}

fn access_chain_resolution_error(result: AccessChainNameResult) -> Diagnostic {
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
        };
        diag!(NameResolution::NamePositionMismatch, (loc, msg))
    } else {
        ice!((
            result.loc(),
            "ICE compiler miscalled access chain resolution error handler"
        ))
    }
}

fn access_chain_incomplete_error(loc: Loc) -> Diagnostic {
    let msg = "Incomplete name in this position. Expected an identifier after '::'";
    diag!(Syntax::InvalidName, (loc, msg))
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
    ) -> Option<E::AttributeValue> {
        use E::AttributeValue_ as EV;
        use P::{AttributeValue_ as PV, LeadingNameAccess_ as LN, NameAccessChain_ as PN};
        Some(sp(
            loc,
            match avalue_ {
                PV::Value(v) => EV::Value(value(context, v)?),
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
                    if context.module_members.get(&mident).is_none() {
                        context.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (ident_loc, format!("Unbound module '{}'", mident))
                        ));
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
                            if context.module_members.get(&mident).is_none() {
                                context.add_diag(diag!(
                                    NameResolution::UnboundModule,
                                    (ident_loc, format!("Unbound module '{}'", mident))
                                ));
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
                            let mident =
                                sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2.name)));
                            if context.module_members.get(&mident).is_none() {
                                context.add_diag(diag!(
                                    NameResolution::UnboundModule,
                                    (ident_loc, format!("Unbound module '{}'", mident))
                                ));
                            }
                            EV::Module(mident)
                        }
                        _ => EV::ModuleAccess(
                            self.name_access_chain_to_module_access(
                                context,
                                Access::Type,
                                sp(ident_loc, PN::Path(path)),
                            )?
                            .access,
                        ),
                    }
                }
                PV::ModuleAccess(ma) => EV::ModuleAccess(
                    self.name_access_chain_to_module_access(context, Access::Type, ma)?
                        .access,
                ),
            },
        ))
    }

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        sp!(loc, ptn_): P::NameAccessChain,
    ) -> Option<ModuleAccessResult> {
        use E::ModuleAccess_ as EN;
        use P::{LeadingNameAccess_ as LN, NameAccessChain_ as PN};

        let tn_: ModuleAccessResult = match (access, ptn_) {
            (Access::Pattern, _) => {
                context.add_diag(ice!((
                    loc,
                    "Attempted to expand a variant with the legacy path expander"
                )));
                return None;
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
                make_access_result(sp(name.loc, access), tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro))
                if is_valid_datatype_or_constant_name(name.value.as_str()) =>
            {
                self.ide_autocomplete_suggestion(context, loc);
                let access = match self.aliases.member_alias_get(&name) {
                    Some((mident, mem)) => EN::ModuleAccess(mident, mem),
                    None => EN::Name(name),
                };
                make_access_result(sp(name.loc, access), tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro)) => {
                self.ide_autocomplete_suggestion(context, loc);
                make_access_result(sp(name.loc, EN::Name(name)), tyargs, is_macro)
            }
            (Access::Module, single_entry!(_name, _tyargs, _is_macro)) => {
                context.add_diag(ice!((
                    loc,
                    "ICE path resolution produced an impossible path for a module"
                )));
                return None;
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
                        let diag = unexpected_address_module_error(loc, *aloc, access);
                        context.add_diag(diag);
                        return None;
                    }
                    (sp!(_aloc, LN::GlobalAddress(_)), [_]) => {
                        let mut diag: Diagnostic = create_feature_error(
                            context.env.edition(None), // We already know we are failing, so no package.
                            FeatureGate::Move2024Paths,
                            loc,
                        );
                        diag.add_secondary_label((
                            loc,
                            "Paths that start with `::` are not valid in legacy move.",
                        ));
                        context.add_diag(diag);
                        return None;
                    }
                    // Others
                    (sp!(_, LN::Name(n1)), [n2]) => {
                        self.ide_autocomplete_suggestion(context, n1.loc);
                        if let Some(mident) = self.aliases.module_alias_get(n1) {
                            let n2_name = n2.name;
                            let (tyargs, is_macro) = if !path.has_tyargs_last() {
                                let mut diag = diag!(
                                    Syntax::InvalidName,
                                    (path.tyargs_loc().unwrap(), "Invalid type argument position")
                                );
                                diag.add_note(
                                    "Type arguments may only be used with module members",
                                );
                                context.add_diag(diag);
                                (None, path.is_macro())
                            } else {
                                (path.take_tyargs(), path.is_macro())
                            };
                            make_access_result(
                                sp(loc, EN::ModuleAccess(mident, n2_name)),
                                tyargs,
                                is_macro.copied(),
                            )
                        } else {
                            context.add_diag(diag!(
                                NameResolution::UnboundModule,
                                (n1.loc, format!("Unbound module alias '{}'", n1))
                            ));
                            return None;
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
                            let mut diag = diag!(
                                Syntax::InvalidName,
                                (path.tyargs_loc().unwrap(), "Invalid type argument position")
                            );
                            diag.add_note("Type arguments may only be used with module members");
                            context.add_diag(diag);
                            (None, path.is_macro())
                        } else {
                            (path.take_tyargs(), path.is_macro())
                        };
                        make_access_result(sp(loc, access), tyargs, is_macro.copied())
                    }
                    (_ln, []) => {
                        let diag = ice!((loc, "Found a root path with no additional entries"));
                        context.add_diag(diag);
                        return None;
                    }
                    (ln, [_n1, _n2, ..]) => {
                        self.ide_autocomplete_suggestion(context, ln.loc);
                        let mut diag = diag!(Syntax::InvalidName, (loc, "Too many name segments"));
                        diag.add_note("Names may only have 0, 1, or 2 segments separated by '::'");
                        context.add_diag(diag);
                        return None;
                    }
                }
            }
        };
        Some(tn_)
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, pn_): P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        use P::NameAccessChain_ as PN;
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
                        context.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (
                                single.name.loc,
                                format!("Unbound module alias '{}'", single.name)
                            ),
                        ));
                        None
                    }
                    Some(mident) => Some(mident),
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
                        Some(module_ident(context, sp(loc, pmident_)))
                    }
                    // Error cases
                    (_ln, []) => {
                        context.add_diag(ice!((loc, "Found path with no path entries")));
                        None
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
                        context.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                                if path.entries.len() < 3 {
                                    (m.name.loc, "Unexpected module member access. Expected a module identifier only")
                                } else {
                                    (loc, "Unexpected access. Expected a module identifier only")
                                }
                        ));
                        None
                    }
                }
            }
        }
    }

    fn ide_autocomplete_suggestion(&mut self, context: &mut DefnContext, loc: Loc) {
        if context.env.ide_mode() && matches!(context.target_kind, P::TargetKind::Source { .. }) {
            let mut info = AliasAutocompleteInfo::new();
            for (name, addr) in context.named_address_mapping.unwrap().iter() {
                info.addresses.insert(*name, *addr);
            }
            for (_, name, (_, mident)) in self.aliases.modules.iter() {
                info.modules.insert(*name, *mident);
            }
            for (_, name, (_, (mident, member))) in self.aliases.members.iter() {
                info.members.insert((*name, *mident, *member));
            }
            let annotation = IDEAnnotation::PathAutocompleteInfo(Box::new(info));
            context.add_ide_annotation(loc, annotation)
        }
    }
}

fn unexpected_address_module_error(loc: Loc, nloc: Loc, access: Access) -> Diagnostic {
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
                (nloc, "Name location")
            )
        }
    };
    let unexpected_msg = format!(
        "Unexpected module identifier. A module identifier is not a valid {}",
        case
    );
    diag!(
        NameResolution::NamePositionMismatch,
        (loc, unexpected_msg),
        (nloc, "Expected a module name".to_owned()),
    )
}
