// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::{codes::WarningFilter, Diagnostic, WarningFilters},
    editions::{create_feature_error, FeatureGate},
    expansion::{
        aliases::{
            AliasEntry, AliasMap, AliasMapBuilder, AliasSet, ParserExplicitUseFun, UseFunsBuilder,
        },
        ast::{self as E, Address, Fields, ModuleIdent, ModuleIdent_},
        byte_string, hex_string, legacy_aliases,
    },
    parser::ast::{
        self as P, Ability, BlockLabel, ConstantName, DatatypeName, Field, FieldBindings,
        FunctionName, ModuleName, Mutability, Var, VariantName,
    },
    shared::{known_attributes::AttributePosition, unique_map::UniqueMap, *},
    FullyCompiledProgram,
};
use move_command_line_common::parser::{parse_u16, parse_u256, parse_u32};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    iter::IntoIterator,
};

//**************************************************************************************************
// Context
//**************************************************************************************************

type ModuleMembers = BTreeMap<Name, ModuleMemberKind>;

// NB: We carry a few things separately because we need to split them out during path resolution to
// allow for dynamic behavior during that resolution. This dynamic behavior allows us to reuse the
// majority of the pass while swapping out how we handle paths and aliases for Move 2024 versus
// legacy.

struct DefnContext<'env, 'map> {
    named_address_mapping: Option<&'map NamedAddressMap>,
    module_members: UniqueMap<ModuleIdent, ModuleMembers>,
    env: &'env mut CompilationEnv,
    address_conflicts: BTreeSet<Symbol>,
}

struct Context<'env, 'map> {
    defn_context: DefnContext<'env, 'map>,
    address: Option<Address>,
    is_source_definition: bool,
    current_package: Option<Symbol>,
    // Cached warning filters for all available prefixes. Used by non-source defs
    // and dependency packages
    all_filter_alls: WarningFilters,
    pub path_expander: Option<Box<dyn PathExpander>>,
}

impl<'env, 'map> Context<'env, 'map> {
    fn new(
        compilation_env: &'env mut CompilationEnv,
        module_members: UniqueMap<ModuleIdent, ModuleMembers>,
        address_conflicts: BTreeSet<Symbol>,
    ) -> Self {
        let mut all_filter_alls = WarningFilters::new_for_dependency();
        for allow in compilation_env.filter_attributes() {
            for f in compilation_env.filter_from_str(FILTER_ALL, *allow) {
                all_filter_alls.add(f);
            }
        }
        let defn_context = DefnContext {
            env: compilation_env,
            named_address_mapping: None,
            address_conflicts,
            module_members,
        };
        Context {
            defn_context,
            address: None,
            is_source_definition: false,
            current_package: None,
            all_filter_alls,
            path_expander: None,
        }
    }

    fn env(&mut self) -> &mut CompilationEnv {
        self.defn_context.env
    }

    fn cur_address(&self) -> &Address {
        self.address.as_ref().unwrap()
    }

    pub fn new_alias_map_builder(&mut self) -> AliasMapBuilder {
        AliasMapBuilder::new(
            self.defn_context
                .env
                .supports_feature(self.current_package, FeatureGate::Move2024Paths),
        )
    }

    /// Pushes a new alias map onto the alias information in the pash expander.
    pub fn push_alias_scope(&mut self, new_scope: AliasMapBuilder) {
        self.path_expander
            .as_mut()
            .unwrap()
            .push_alias_scope(new_scope);
    }

    // Push a number of type parameters onto the alias information in the path expander.
    pub fn push_type_parameters<'a, I: IntoIterator<Item = &'a Name>>(&mut self, tparams: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        self.path_expander
            .as_mut()
            .unwrap()
            .push_type_parameters(tparams.into_iter().collect::<Vec<_>>());
    }

    /// Pops the innermost alias information on the path expander and reports errors for aliases
    /// that were unused Marks implicit use funs as unused
    pub fn pop_alias_scope(&mut self, mut use_funs: Option<&mut E::UseFuns>) {
        let AliasSet { modules, members } = self.path_expander.as_mut().unwrap().pop_alias_scope();
        for alias in modules {
            unused_alias(self, "module", alias)
        }
        for alias in members {
            let use_fun_used_opt = use_funs
                .as_mut()
                .and_then(|use_funs| use_funs.implicit.get_mut(&alias))
                .and_then(|use_fun| match &mut use_fun.kind {
                    E::ImplicitUseFunKind::FunctionDeclaration => None,
                    E::ImplicitUseFunKind::UseAlias { used } => Some(used),
                });
            if let Some(used) = use_fun_used_opt {
                // We do not report the use error if it is a function alias, since these will be
                // reported after method calls are fully resolved
                *used = false;
            } else {
                unused_alias(self, "member", alias)
            }
        }
    }

    pub fn attribute_value(
        &mut self,
        attribute_value: P::AttributeValue,
    ) -> Option<E::AttributeValue> {
        let Context {
            path_expander,
            defn_context: inner_context,
            ..
        } = self;
        path_expander
            .as_mut()
            .unwrap()
            .name_access_chain_to_attribute_value(inner_context, attribute_value)
    }

    pub fn name_access_chain_to_module_access(
        &mut self,
        access: Access,
        chain: P::NameAccessChain,
    ) -> Option<E::ModuleAccess> {
        let Context {
            path_expander,
            defn_context: inner_context,
            ..
        } = self;
        path_expander
            .as_mut()
            .unwrap()
            .name_access_chain_to_module_access(inner_context, access, chain)
    }

    pub fn name_access_chain_to_module_ident(
        &mut self,
        chain: P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        let Context {
            path_expander,
            defn_context: inner_context,
            ..
        } = self;
        path_expander
            .as_mut()
            .unwrap()
            .name_access_chain_to_module_ident(inner_context, chain)
    }

    pub fn spec_deprecated(&mut self, loc: Loc, is_error: bool) {
        self.env().add_diag(diag!(
            if is_error {
                Uncategorized::DeprecatedSpecItem
            } else {
                Uncategorized::DeprecatedWillBeRemoved
            },
            (loc, "Specification blocks are deprecated")
        ));
    }
}

/// We mark named addresses as having a conflict if there is not a bidirectional mapping between
/// the name and its value
fn compute_address_conflicts(
    pre_compiled_lib: Option<&FullyCompiledProgram>,
    prog: &P::Program,
) -> BTreeSet<Symbol> {
    let mut name_to_addr: BTreeMap<Symbol, BTreeSet<AccountAddress>> = BTreeMap::new();
    let mut addr_to_name: BTreeMap<AccountAddress, BTreeSet<Symbol>> = BTreeMap::new();
    let all_addrs = prog.named_address_maps.all().iter().chain(
        pre_compiled_lib
            .iter()
            .flat_map(|pre| pre.parser.named_address_maps.all()),
    );
    for map in all_addrs {
        for (n, addr) in map {
            let n = *n;
            let addr = addr.into_inner();
            name_to_addr.entry(n).or_default().insert(addr);
            addr_to_name.entry(addr).or_default().insert(n);
        }
    }
    let name_to_addr_conflicts = name_to_addr
        .into_iter()
        .filter(|(_, addrs)| addrs.len() > 1)
        .map(|(n, _)| n);
    let addr_to_name_conflicts = addr_to_name
        .into_iter()
        .filter(|(_, addrs)| addrs.len() > 1)
        .flat_map(|(_, ns)| ns.into_iter());
    name_to_addr_conflicts
        .chain(addr_to_name_conflicts)
        .collect()
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &mut CompilationEnv,
    pre_compiled_lib: Option<&FullyCompiledProgram>,
    prog: P::Program,
) -> E::Program {
    let address_conflicts = compute_address_conflicts(pre_compiled_lib, &prog);

    let mut member_computation_context = DefnContext {
        env: compilation_env,
        named_address_mapping: None,
        module_members: UniqueMap::new(),
        address_conflicts,
    };

    let module_members = {
        let mut members = UniqueMap::new();
        all_module_members(
            &mut member_computation_context,
            &prog.named_address_maps,
            &mut members,
            true,
            &prog.source_definitions,
        );
        all_module_members(
            &mut member_computation_context,
            &prog.named_address_maps,
            &mut members,
            true,
            &prog.lib_definitions,
        );
        if let Some(pre_compiled) = pre_compiled_lib {
            assert!(pre_compiled.parser.lib_definitions.is_empty());
            all_module_members(
                &mut member_computation_context,
                &pre_compiled.parser.named_address_maps,
                &mut members,
                false,
                &pre_compiled.parser.source_definitions,
            );
        }
        members
    };

    let address_conflicts = member_computation_context.address_conflicts;

    let mut source_module_map = UniqueMap::new();
    let mut lib_module_map = UniqueMap::new();
    let P::Program {
        named_address_maps,
        source_definitions,
        lib_definitions,
    } = prog;

    let mut context = Context::new(compilation_env, module_members, address_conflicts);

    context.is_source_definition = true;
    for P::PackageDefinition {
        package,
        named_address_map,
        def,
    } in source_definitions
    {
        context.current_package = package;
        let named_address_map = named_address_maps.get(named_address_map);
        if context
            .env()
            .supports_feature(package, FeatureGate::Move2024Paths)
        {
            let mut path_expander = Move2024PathExpander::new();

            let aliases = named_addr_map_to_alias_map_builder(&mut context, named_address_map);
            path_expander.push_alias_scope(aliases);

            context.defn_context.named_address_mapping = Some(named_address_map);
            context.path_expander = Some(Box::new(path_expander));
            definition(&mut context, &mut source_module_map, package, def);
            context.pop_alias_scope(None); // Handle unused addresses in this case
            context.path_expander = None;
        } else {
            context.defn_context.named_address_mapping = Some(named_address_map);
            context.path_expander = Some(Box::new(LegacyPathExpander::new()));
            definition(&mut context, &mut source_module_map, package, def);
            context.path_expander = None;
        }
    }

    context.is_source_definition = false;
    for P::PackageDefinition {
        package,
        named_address_map,
        def,
    } in lib_definitions
    {
        context.current_package = package;
        let named_address_map = named_address_maps.get(named_address_map);
        if context
            .env()
            .supports_feature(package, FeatureGate::Move2024Paths)
        {
            let mut path_expander = Move2024PathExpander::new();

            let aliases = named_addr_map_to_alias_map_builder(&mut context, named_address_map);
            path_expander.push_alias_scope(aliases);

            context.defn_context.named_address_mapping = Some(named_address_map);
            context.path_expander = Some(Box::new(path_expander));
            definition(&mut context, &mut lib_module_map, package, def);
            context.pop_alias_scope(None); // Handle unused addresses in this case
            context.path_expander = None;
        } else {
            context.defn_context.named_address_mapping = Some(named_address_map);
            context.path_expander = Some(Box::new(LegacyPathExpander::new()));
            definition(&mut context, &mut lib_module_map, package, def);
            context.path_expander = None;
        }
    }

    context.current_package = None;

    // Finalization
    //
    for (mident, module) in lib_module_map {
        if let Err((mident, old_loc)) = source_module_map.add(mident, module) {
            if !context.env().flags().sources_shadow_deps() {
                duplicate_module(&mut context, &source_module_map, mident, old_loc)
            }
        }
    }
    let module_map = source_module_map;

    super::primitive_definers::modules(context.env(), pre_compiled_lib, &module_map);
    E::Program {
        modules: module_map,
    }
}

fn definition(
    context: &mut Context,
    module_map: &mut UniqueMap<ModuleIdent, E::ModuleDefinition>,
    package_name: Option<Symbol>,
    def: P::Definition,
) {
    match def {
        P::Definition::Module(mut m) => {
            let module_paddr = std::mem::take(&mut m.address);
            let module_addr = module_paddr.map(|addr| {
                let address = top_level_address(
                    &mut context.defn_context,
                    /* suggest_declaration */ true,
                    addr,
                );
                sp(addr.loc, address)
            });
            module(context, module_map, package_name, module_addr, m)
        }
        P::Definition::Address(a) => {
            let addr = top_level_address(
                &mut context.defn_context,
                /* suggest_declaration */ false,
                a.addr,
            );
            for mut m in a.modules {
                let module_addr = check_module_address(context, a.loc, addr, &mut m);
                module(context, module_map, package_name, Some(module_addr), m)
            }
        }
    }
}

// Access a top level address as declared, not affected by any aliasing/shadowing
fn top_level_address(
    context: &mut DefnContext,
    suggest_declaration: bool,
    ln: P::LeadingNameAccess,
) -> Address {
    top_level_address_(
        context,
        context.named_address_mapping.as_ref().unwrap(),
        suggest_declaration,
        ln,
    )
}

fn top_level_address_(
    context: &mut DefnContext,
    named_address_mapping: &NamedAddressMap,
    suggest_declaration: bool,
    ln: P::LeadingNameAccess,
) -> Address {
    let name_res = check_valid_address_name(context, &ln);
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => {
            debug_assert!(name_res.is_ok());
            Address::anonymous(loc, bytes)
        }
        P::LeadingNameAccess_::GlobalAddress(name) => {
            context.env.add_diag(diag!(
                Syntax::InvalidAddress,
                (loc, "Top-level addresses cannot start with '::'")
            ));
            Address::NamedUnassigned(name)
        }
        P::LeadingNameAccess_::Name(name) => {
            match named_address_mapping.get(&name.value).copied() {
                Some(addr) => make_address(context, name, loc, addr),
                None => {
                    if name_res.is_ok() {
                        context.env.add_diag(address_without_value_error(
                            suggest_declaration,
                            loc,
                            &name,
                        ));
                    }
                    Address::NamedUnassigned(name)
                }
            }
        }
    }
}

fn address_without_value_error(suggest_declaration: bool, loc: Loc, n: &Name) -> Diagnostic {
    let mut msg = format!("address '{}' is not assigned a value", n);
    if suggest_declaration {
        msg = format!(
            "{}. Try assigning it a value when calling the compiler",
            msg,
        )
    }
    diag!(NameResolution::AddressWithoutValue, (loc, msg))
}

fn make_address(
    context: &mut DefnContext,
    name: Name,
    loc: Loc,
    value: NumericalAddress,
) -> Address {
    Address::Numerical {
        name: Some(name),
        value: sp(loc, value),
        name_conflict: context.address_conflicts.contains(&name.value),
    }
}

fn module_ident(context: &mut DefnContext, sp!(loc, mident_): P::ModuleIdent) -> ModuleIdent {
    let P::ModuleIdent_ {
        address: ln,
        module,
    } = mident_;
    let addr = top_level_address(context, /* suggest_declaration */ false, ln);
    sp(loc, ModuleIdent_::new(addr, module))
}

fn check_module_address(
    context: &mut Context,
    loc: Loc,
    addr: Address,
    m: &mut P::ModuleDefinition,
) -> Spanned<Address> {
    let module_address = std::mem::take(&mut m.address);
    match module_address {
        Some(other_paddr) => {
            let other_loc = other_paddr.loc;
            let other_addr = top_level_address(
                &mut context.defn_context,
                /* suggest_declaration */ true,
                other_paddr,
            );
            let msg = if addr == other_addr {
                "Redundant address specification"
            } else {
                "Multiple addresses specified for module"
            };
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (other_loc, msg),
                (loc, "Address previously specified here")
            ));
            sp(other_loc, other_addr)
        }
        None => sp(loc, addr),
    }
}

fn duplicate_module(
    context: &mut Context,
    module_map: &UniqueMap<ModuleIdent, E::ModuleDefinition>,
    mident: ModuleIdent,
    old_loc: Loc,
) {
    let old_mident = module_map.get_key(&mident).unwrap();
    let dup_msg = format!("Duplicate definition for module '{}'", mident);
    let prev_msg = format!("Module previously defined here, with '{}'", old_mident);
    context.env().add_diag(diag!(
        Declarations::DuplicateItem,
        (mident.loc, dup_msg),
        (old_loc, prev_msg),
    ))
}

fn module(
    context: &mut Context,
    module_map: &mut UniqueMap<ModuleIdent, E::ModuleDefinition>,
    package_name: Option<Symbol>,
    module_address: Option<Spanned<Address>>,
    module_def: P::ModuleDefinition,
) {
    assert!(context.address.is_none());
    let (mident, mod_) = module_(context, package_name, module_address, module_def);
    if let Err((mident, old_loc)) = module_map.add(mident, mod_) {
        duplicate_module(context, module_map, mident, old_loc)
    }
    context.address = None
}

fn set_sender_address(
    context: &mut Context,
    module_name: &ModuleName,
    sender: Option<Spanned<Address>>,
) {
    context.address = Some(match sender {
        Some(sp!(_, addr)) => addr,
        None => {
            let loc = module_name.loc();
            let msg = format!(
                "Invalid module declaration. The module does not have a specified address. Either \
                 declare it inside of an 'address <address> {{' block or declare it with an \
                 address 'module <address>::{}''",
                module_name
            );
            context
                .env()
                .add_diag(diag!(Declarations::InvalidModule, (loc, msg)));
            Address::anonymous(loc, NumericalAddress::DEFAULT_ERROR_ADDRESS)
        }
    })
}

fn module_(
    context: &mut Context,
    package_name: Option<Symbol>,
    module_address: Option<Spanned<Address>>,
    mdef: P::ModuleDefinition,
) -> (ModuleIdent, E::ModuleDefinition) {
    let P::ModuleDefinition {
        attributes,
        loc,
        address,
        is_spec_module: _,
        name,
        members,
    } = mdef;
    let attributes = flatten_attributes(context, AttributePosition::Module, attributes);
    let mut warning_filter = module_warning_filter(context, &attributes);
    let config = context.env().package_config(package_name);
    warning_filter.union(&config.warning_filter);

    context
        .env()
        .add_warning_filter_scope(warning_filter.clone());
    assert!(context.address.is_none());
    assert!(address.is_none());
    set_sender_address(context, &name, module_address);
    let _ = check_restricted_name_all_cases(&mut context.defn_context, NameCase::Module, &name.0);
    if name.value().starts_with(|c| c == '_') {
        let msg = format!(
            "Invalid module name '{}'. Module names cannot start with '_'",
            name,
        );
        context
            .env()
            .add_diag(diag!(Declarations::InvalidName, (name.loc(), msg)));
    }

    let name_loc = name.0.loc;
    let current_module = sp(name_loc, ModuleIdent_::new(*context.cur_address(), name));

    let mut new_scope = context.new_alias_map_builder();
    let mut use_funs_builder = UseFunsBuilder::new();
    module_self_aliases(&mut new_scope, &current_module);
    let members = members
        .into_iter()
        .filter_map(|member| {
            aliases_from_member(
                context,
                &mut new_scope,
                &mut use_funs_builder,
                &current_module,
                member,
            )
        })
        .collect::<Vec<_>>();
    context.push_alias_scope(new_scope);

    let mut friends = UniqueMap::new();
    let mut functions = UniqueMap::new();
    let mut constants = UniqueMap::new();
    let mut structs = UniqueMap::new();
    let mut enums = UniqueMap::new();
    for member in members {
        match member {
            P::ModuleMember::Use(_) => unreachable!(),
            P::ModuleMember::Friend(f) => friend(context, &mut friends, f),
            P::ModuleMember::Function(mut f) => {
                if !context.is_source_definition {
                    f.body.value = P::FunctionBody_::Native
                }
                function(
                    context,
                    Some((current_module, &mut use_funs_builder)),
                    &mut functions,
                    f,
                )
            }
            P::ModuleMember::Constant(c) => constant(context, &mut constants, c),
            P::ModuleMember::Struct(s) => struct_def(context, &mut structs, s),
            P::ModuleMember::Enum(e) => enum_def(context, &mut enums, e),
            P::ModuleMember::Spec(s) => context.spec_deprecated(s.loc, /* is_error */ false),
        }
    }
    let mut use_funs = use_funs(context, use_funs_builder);
    check_visibility_modifiers(context, &functions, &friends, package_name);

    context.pop_alias_scope(Some(&mut use_funs));

    let def = E::ModuleDefinition {
        package_name,
        attributes,
        loc,
        use_funs,
        is_source_module: context.is_source_definition,
        friends,
        structs,
        enums,
        constants,
        functions,
        warning_filter,
    };
    context.env().pop_warning_filter_scope();
    (current_module, def)
}

fn check_visibility_modifiers(
    context: &mut Context,
    functions: &UniqueMap<FunctionName, E::Function>,
    friends: &UniqueMap<ModuleIdent, E::Friend>,
    package_name: Option<Symbol>,
) {
    let mut friend_usage = friends.iter().next().map(|(_, _, friend)| friend.loc);
    let mut public_package_usage = None;
    for (_, _, function) in functions {
        match function.visibility {
            E::Visibility::Friend(loc) if friend_usage.is_none() => {
                friend_usage = Some(loc);
            }
            E::Visibility::Package(loc) => {
                context
                    .env()
                    .check_feature(FeatureGate::PublicPackage, package_name, loc);
                public_package_usage = Some(loc);
            }
            _ => (),
        }
    }

    // Emit any errors.
    if public_package_usage.is_some() && friend_usage.is_some() {
        let friend_error_msg = format!(
            "Cannot define 'friend' modules and use '{}' visibility in the same module",
            E::Visibility::PACKAGE
        );
        let package_definition_msg = format!("'{}' visibility used here", E::Visibility::PACKAGE);
        for (_, _, friend) in friends {
            context.env().add_diag(diag!(
                Declarations::InvalidVisibilityModifier,
                (friend.loc, friend_error_msg.clone()),
                (
                    public_package_usage.unwrap(),
                    package_definition_msg.clone()
                )
            ));
        }
        let package_error_msg = format!(
            "Cannot mix '{}' and '{}' visibilities in the same module",
            E::Visibility::PACKAGE_IDENT,
            E::Visibility::FRIEND_IDENT
        );
        let friend_error_msg = format!(
            "Cannot mix '{}' and '{}' visibilities in the same module",
            E::Visibility::FRIEND_IDENT,
            E::Visibility::PACKAGE_IDENT
        );
        for (_, _, function) in functions {
            match function.visibility {
                E::Visibility::Friend(loc) => {
                    context.env().add_diag(diag!(
                        Declarations::InvalidVisibilityModifier,
                        (loc, friend_error_msg.clone()),
                        (
                            public_package_usage.unwrap(),
                            package_definition_msg.clone()
                        )
                    ));
                }
                E::Visibility::Package(loc) => {
                    context.env().add_diag(diag!(
                        Declarations::InvalidVisibilityModifier,
                        (loc, package_error_msg.clone()),
                        (
                            friend_usage.unwrap(),
                            &format!("'{}' visibility used here", E::Visibility::FRIEND_IDENT)
                        )
                    ));
                }
                _ => {}
            }
        }
    }
}

fn flatten_attributes(
    context: &mut Context,
    attr_position: AttributePosition,
    attributes: Vec<P::Attributes>,
) -> E::Attributes {
    let all_attrs = attributes
        .into_iter()
        .flat_map(|attrs| attrs.value)
        .flat_map(|attr| attribute(context, attr_position, attr))
        .collect::<Vec<_>>();
    unique_attributes(context, attr_position, false, all_attrs)
}

fn unique_attributes(
    context: &mut Context,
    attr_position: AttributePosition,
    is_nested: bool,
    attributes: impl IntoIterator<Item = E::Attribute>,
) -> E::Attributes {
    let mut attr_map = UniqueMap::new();
    for sp!(loc, attr_) in attributes {
        let sp!(nloc, sym) = match &attr_ {
            E::Attribute_::Name(n)
            | E::Attribute_::Assigned(n, _)
            | E::Attribute_::Parameterized(n, _) => *n,
        };
        let name_ = match known_attributes::KnownAttribute::resolve(sym) {
            None => E::AttributeName_::Unknown(sym),
            Some(known) => {
                debug_assert!(known.name() == sym.as_str());
                if is_nested {
                    let msg = "Known attribute '{}' is not expected in a nested attribute position";
                    context
                        .env()
                        .add_diag(diag!(Declarations::InvalidAttribute, (nloc, msg)));
                    continue;
                }

                let expected_positions = known.expected_positions();
                if !expected_positions.contains(&attr_position) {
                    let msg = format!(
                        "Known attribute '{}' is not expected with a {}",
                        known.name(),
                        attr_position
                    );
                    let all_expected = expected_positions
                        .iter()
                        .map(|p| format!("{}", p))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let expected_msg = format!(
                        "Expected to be used with one of the following: {}",
                        all_expected
                    );
                    context.env().add_diag(diag!(
                        Declarations::InvalidAttribute,
                        (nloc, msg),
                        (nloc, expected_msg)
                    ));
                    continue;
                }
                E::AttributeName_::Known(known)
            }
        };
        if let Err((_, old_loc)) = attr_map.add(sp(nloc, name_), sp(loc, attr_)) {
            let msg = format!("Duplicate attribute '{}' attached to the same item", name_);
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, msg),
                (old_loc, "Attribute previously given here"),
            ));
        }
    }
    attr_map
}

fn attribute(
    context: &mut Context,
    attr_position: AttributePosition,
    sp!(loc, attribute_): P::Attribute,
) -> Option<E::Attribute> {
    use E::Attribute_ as EA;
    use P::Attribute_ as PA;
    Some(sp(
        loc,
        match attribute_ {
            PA::Name(n) => EA::Name(n),
            PA::Assigned(n, v) => EA::Assigned(n, Box::new(context.attribute_value(*v)?)),
            PA::Parameterized(n, sp!(_, pattrs_)) => {
                let attrs = pattrs_
                    .into_iter()
                    .map(|a| attribute(context, attr_position, a))
                    .collect::<Option<Vec<_>>>()?;
                EA::Parameterized(n, unique_attributes(context, attr_position, true, attrs))
            }
        },
    ))
}

/// Like warning_filter, but it will filter _all_ warnings for non-source definitions (or for any
/// dependency packages)
fn module_warning_filter(
    context: &mut Context,
    attributes: &UniqueMap<E::AttributeName, E::Attribute>,
) -> WarningFilters {
    let filters = warning_filter(context, attributes);
    let is_dep = !context.is_source_definition || {
        let pkg = context.current_package;
        context.env().package_config(pkg).is_dependency
    };
    if is_dep {
        // For dependencies (non source defs or package deps), we check the filters for errors
        // but then throw them away and actually ignore _all_ warnings
        context.all_filter_alls.clone()
    } else {
        filters
    }
}

fn warning_filter(
    context: &mut Context,
    attributes: &UniqueMap<E::AttributeName, E::Attribute>,
) -> WarningFilters {
    use crate::diagnostics::codes::Category;
    use known_attributes::DiagnosticAttribute;
    let mut warning_filters = WarningFilters::new_for_source();
    let filter_attribute_names = context.env().filter_attributes().clone();
    for allow in filter_attribute_names {
        let Some(attr) = attributes.get_(&allow) else {
            continue;
        };
        let inners = match &attr.value {
            E::Attribute_::Parameterized(_, inner) if !inner.is_empty() => inner,
            _ => {
                let msg = format!(
                    "Expected list of warnings, e.g. '{}({})'",
                    DiagnosticAttribute::ALLOW,
                    WarningFilter::Category {
                        prefix: None,
                        category: Category::UnusedItem as u8,
                        name: Some(FILTER_UNUSED)
                    }
                    .to_str()
                    .unwrap(),
                );
                context
                    .env()
                    .add_diag(diag!(Attributes::InvalidValue, (attr.loc, msg)));
                continue;
            }
        };
        for (inner_attr_loc, _, inner_attr) in inners {
            let sp!(_, name_) = match inner_attr.value {
                E::Attribute_::Name(n) => n,
                E::Attribute_::Assigned(n, _) | E::Attribute_::Parameterized(n, _) => {
                    let msg = format!(
                        "Expected a stand alone warning filter identifier, e.g. '{}({})'",
                        DiagnosticAttribute::ALLOW,
                        n
                    );
                    context
                        .env()
                        .add_diag(diag!(Attributes::InvalidValue, (inner_attr_loc, msg)));
                    n
                }
            };
            let filters = context.env().filter_from_str(name_, allow);
            if filters.is_empty() {
                let msg = format!("Unknown warning filter '{name_}'");
                context
                    .env()
                    .add_diag(diag!(Attributes::InvalidValue, (attr.loc, msg)));
                continue;
            };
            for f in filters {
                warning_filters.add(f);
            }
        }
    }
    warning_filters
}

//**************************************************************************************************
// Name Access Chain (Path) Resolution
//**************************************************************************************************

#[derive(Clone, Copy)]
enum Access {
    Type,
    ApplyNamed,
    ApplyPositional,
    Term,
    Variant,
    Module, // Just used for errors
}

// This trait describes the commands available to handle alias scopes and expanding name access
// chains. This is used to model both legacy and modern path expansion.

trait PathExpander {
    // Push a new innermost alias scope
    fn push_alias_scope(&mut self, new_scope: AliasMapBuilder);

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
    ) -> Option<E::ModuleAccess>;

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        name_chain: P::NameAccessChain,
    ) -> Option<E::ModuleIdent>;
}

// -----------------------------------------------
// Legacy Implementation

struct LegacyPathExpander {
    aliases: legacy_aliases::AliasMap,
    old_alias_maps: Vec<legacy_aliases::OldAliasMap>,
}

impl LegacyPathExpander {
    fn new() -> LegacyPathExpander {
        LegacyPathExpander {
            aliases: legacy_aliases::AliasMap::new(),
            old_alias_maps: vec![],
        }
    }
}

impl PathExpander for LegacyPathExpander {
    fn push_alias_scope(&mut self, new_scope: AliasMapBuilder) {
        self.old_alias_maps
            .push(self.aliases.add_and_shadow_all(new_scope));
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
                PV::ModuleAccess(
                    sp!(ident_loc, PN::Two(sp!(aloc, LN::AnonymousAddress(a)), n)),
                ) => {
                    let addr = Address::anonymous(aloc, a);
                    let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n)));
                    if context.module_members.get(&mident).is_none() {
                        context.env.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (ident_loc, format!("Unbound module '{}'", mident))
                        ));
                    }
                    EV::Module(mident)
                }
                // bit wonky, but this is the only spot currently where modules and expressions exist
                // in the same namespace.
                // TODO consider if we want to just force all of these checks into the well-known
                // attribute setup
                PV::ModuleAccess(sp!(ident_loc, PN::One(n)))
                    if self.aliases.module_alias_get(&n).is_some() =>
                {
                    let sp!(_, mident_) = self.aliases.module_alias_get(&n).unwrap();
                    let mident = sp(ident_loc, mident_);
                    if context.module_members.get(&mident).is_none() {
                        context.env.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (ident_loc, format!("Unbound module '{}'", mident))
                        ));
                    }
                    EV::Module(mident)
                }
                PV::ModuleAccess(sp!(ident_loc, PN::Two(sp!(aloc, LN::Name(n1)), n2)))
                    if context
                        .named_address_mapping
                        .as_ref()
                        .map(|m| m.contains_key(&n1.value))
                        .unwrap_or(false) =>
                {
                    let addr = top_level_address(
                        context,
                        /* suggest_declaration */ false,
                        sp(aloc, LN::Name(n1)),
                    );
                    let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2)));
                    if context.module_members.get(&mident).is_none() {
                        context.env.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (ident_loc, format!("Unbound module '{}'", mident))
                        ));
                    }
                    EV::Module(mident)
                }
                PV::ModuleAccess(ma) => EV::ModuleAccess(self.name_access_chain_to_module_access(
                    context,
                    Access::Type,
                    ma,
                )?),
            },
        ))
    }

    fn name_access_chain_to_module_access(
        &mut self,
        context: &mut DefnContext,
        access: Access,
        sp!(loc, ptn_): P::NameAccessChain,
    ) -> Option<E::ModuleAccess> {
        use E::ModuleAccess_ as EN;
        use P::{LeadingNameAccess_ as LN, NameAccessChain_ as PN};

        let tn_ = match (access, ptn_) {
            (Access::ApplyPositional, PN::One(n))
            | (Access::ApplyNamed, PN::One(n))
            | (Access::Type, PN::One(n)) => match self.aliases.member_alias_get(&n) {
                Some((mident, mem)) => EN::ModuleAccess(mident, mem),
                None => EN::Name(n),
            },
            (Access::Term, PN::One(n))
                if is_valid_struct_constant_or_schema_name(n.value.as_str()) =>
            {
                match self.aliases.member_alias_get(&n) {
                    Some((mident, mem)) => EN::ModuleAccess(mident, mem),
                    None => EN::Name(n),
                }
            }
            (Access::Term | Access::Variant, PN::One(n)) => EN::Name(n),
            (Access::Module, PN::One(_n)) => panic!("ICE invalid resolution"),
            (_, PN::Two(sp!(nloc, LN::AnonymousAddress(_)), _)) => {
                context
                    .env
                    .add_diag(unexpected_address_module_error(loc, nloc, access));
                return None;
            }

            (_, PN::Two(sp!(_, LN::Name(n1)), n2)) => match self.aliases.module_alias_get(&n1) {
                None => {
                    context.env.add_diag(diag!(
                        NameResolution::UnboundModule,
                        (n1.loc, format!("Unbound module alias '{}'", n1))
                    ));
                    return None;
                }
                Some(mident) => EN::ModuleAccess(mident, n2),
            },
            (_, PN::Two(sp!(eloc, LN::GlobalAddress(_)), _)) => {
                let mut diag: Diagnostic = create_feature_error(
                    context.env.edition(None), // We already know we are failing, so no package.
                    FeatureGate::Move2024Paths,
                    eloc,
                );
                diag.add_secondary_label((
                    eloc,
                    "Paths that start with `::` are not valid in legacy move.",
                ));
                context.env.add_diag(diag);
                return None;
            }
            (_, PN::Three(sp!(ident_loc, (ln, n2)), n3)) => {
                let addr = top_level_address(context, /* suggest_declaration */ false, ln);
                let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2)));
                EN::ModuleAccess(mident, n3)
            }
            (_, PN::Four(sp!(ident_loc, (ln, n)), _, _)) => {
                // Process the module ident just for errors
                let pmident_ = P::ModuleIdent_ {
                    address: ln,
                    module: ModuleName(n),
                };
                let _ = module_ident(context, sp(ident_loc, pmident_));
                context.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (
                        loc,
                        "Unexpected path of length four. Expected a module member only",
                    )
                ));
                return None;
            }
        };
        Some(sp(loc, tn_))
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, pn_): P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        use P::NameAccessChain_ as PN;
        match pn_ {
            PN::One(name) => match self.aliases.module_alias_get(&name) {
                None => {
                    context.env.add_diag(diag!(
                        NameResolution::UnboundModule,
                        (name.loc, format!("Unbound module alias '{}'", name)),
                    ));
                    None
                }
                Some(mident) => Some(mident),
            },
            PN::Two(ln, n) => {
                let pmident_ = P::ModuleIdent_ {
                    address: ln,
                    module: ModuleName(n),
                };
                Some(module_ident(context, sp(loc, pmident_)))
            }
            PN::Three(sp!(ident_loc, (ln, n)), mem) => {
                // Process the module ident just for errors
                let pmident_ = P::ModuleIdent_ {
                    address: ln,
                    module: ModuleName(n),
                };
                let _ = module_ident(context, sp(ident_loc, pmident_));
                context.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (
                        mem.loc,
                        "Unexpected module member access. Expected a module identifier only",
                    )
                ));
                None
            }
            PN::Four(sp!(ident_loc, (ln, n)), _, _) => {
                // Process the module ident just for errors
                let pmident_ = P::ModuleIdent_ {
                    address: ln,
                    module: ModuleName(n),
                };
                let _ = module_ident(context, sp(ident_loc, pmident_));
                context.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (
                        loc,
                        "Unexpected path of length four. Expected a module identifier only",
                    )
                ));
                None
            }
        }
    }
}

fn unexpected_address_module_error(loc: Loc, nloc: Loc, access: Access) -> Diagnostic {
    let case = match access {
        Access::Type | Access::ApplyNamed | Access::ApplyPositional => "type",
        Access::Term => "expression",
        Access::Variant => "pattern",
        Access::Module => panic!("ICE expected a module name and got one, but hit error"),
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

// -----------------------------------------------
// Move 2024 Implementation

struct Move2024PathExpander {
    aliases: AliasMap,
}

#[derive(Debug)]
enum AccessChainResult {
    ModuleAccess(Loc, E::ModuleAccess_),
    Variant(Loc, E::ModuleAccess_),
    Address(Loc, E::Address),
    ModuleIdent(Loc, E::ModuleIdent),
    UnresolvedName(Loc, Name),
    ResolutionFailure(Box<AccessChainResult>, AccessChainFailure),
}

#[derive(Debug)]
enum AccessChainFailure {
    UnresolvedAlias(Name),
    InvalidKind(String),
}

impl Move2024PathExpander {
    fn new() -> Move2024PathExpander {
        Move2024PathExpander {
            aliases: AliasMap::new(),
        }
    }

    fn resolve_root(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, name): P::LeadingNameAccess,
    ) -> AccessChainResult {
        use AccessChainFailure::*;
        use AccessChainResult::*;
        use P::LeadingNameAccess_ as LN;
        match name {
            LN::AnonymousAddress(address) => Address(loc, E::Address::anonymous(loc, address)),
            LN::GlobalAddress(name) => {
                if let Some(address) = context
                    .named_address_mapping
                    .expect("ICE no named address mapping")
                    .get(&name.value)
                {
                    Address(loc, make_address(context, name, name.loc, *address))
                } else {
                    ResolutionFailure(Box::new(UnresolvedName(loc, name)), UnresolvedAlias(name))
                }
            }
            LN::Name(name) => match self.resolve_name(context, name) {
                result @ UnresolvedName(_, _) => {
                    ResolutionFailure(Box::new(result), UnresolvedAlias(name))
                }
                other => other,
            },
        }
    }

    fn resolve_name(&mut self, context: &mut DefnContext, name: Name) -> AccessChainResult {
        use AccessChainResult::*;
        use E::ModuleAccess_ as EN;
        match self.aliases.get(&name) {
            Some(AliasEntry::Member(mident, sp!(_, mem))) => {
                // We are preserving the name's original location, rather than referring to where
                // the alias was defined. The name represents JUST the member name, though, so we do
                // not change location of the module as we don't have this information.
                // TODO maybe we should also keep the alias reference (or its location)?
                ModuleAccess(name.loc, EN::ModuleAccess(mident, sp(name.loc, mem)))
            }
            Some(AliasEntry::Module(mident)) => {
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
                ModuleIdent(name.loc, sp(name.loc, ModuleIdent_ { address, module }))
            }
            Some(AliasEntry::Address(address)) => {
                Address(name.loc, make_address(context, name, name.loc, address))
            }
            Some(AliasEntry::TypeParam) => panic!("ICE alias map lookup error"),
            None => UnresolvedName(name.loc, name),
        }
    }

    fn resolve_name_access_chain(
        &mut self,
        context: &mut DefnContext,
        sp!(loc, chain): P::NameAccessChain,
    ) -> AccessChainResult {
        use AccessChainFailure::*;
        use AccessChainResult::*;
        use E::ModuleAccess_ as EN;
        use P::NameAccessChain_ as PN;
        match chain {
            PN::One(name) => self.resolve_name(context, name),
            PN::Two(sp!(rloc, root_name), name) => {
                match self.resolve_root(context, sp(rloc, root_name)) {
                    Address(_, address) => {
                        ModuleIdent(loc, sp(loc, ModuleIdent_::new(address, ModuleName(name))))
                    }
                    ModuleIdent(_, mident) => ModuleAccess(loc, EN::ModuleAccess(mident, name)),
                    ModuleAccess(_, EN::ModuleAccess(mident, enum_name)) => {
                        Variant(loc, EN::Variant(sp(rloc, (mident, enum_name)), name))
                    }
                    result @ ModuleAccess(_, _) => {
                        ResolutionFailure(Box::new(result), InvalidKind("an enum".to_string()))
                    }
                    result @ Variant(_, _) => ResolutionFailure(
                        Box::new(result),
                        InvalidKind("a module or module member".to_string()),
                    ),
                    result @ ResolutionFailure(_, _) => result,
                    UnresolvedName(_, _) => panic!("ICE failed in access chain expansion"),
                }
            }
            PN::Three(sp!(ident_loc, (root_name, next_name)), last_name) => {
                match self.resolve_root(context, root_name) {
                    Address(_, address) => {
                        let mident =
                            sp(ident_loc, ModuleIdent_::new(address, ModuleName(next_name)));
                        ModuleAccess(loc, EN::ModuleAccess(mident, last_name))
                    }
                    ModuleIdent(_, mident) => ModuleAccess(
                        loc,
                        EN::Variant(sp(ident_loc, (mident, next_name)), last_name),
                    ),
                    result @ (ModuleAccess(_, _) | Variant(_, _)) => ResolutionFailure(
                        Box::new(result),
                        InvalidKind("an address or module".to_string()),
                    ),
                    result @ ResolutionFailure(_, _) => result,
                    UnresolvedName(_, _) => panic!("ICE failed in access chain expansion"),
                }
            }
            PN::Four(sp!(ident_loc, (root_name, next_name)), access_name, last_name) => {
                match self.resolve_root(context, root_name) {
                    Address(_, address) => {
                        let mident =
                            sp(ident_loc, ModuleIdent_::new(address, ModuleName(next_name)));
                        ModuleAccess(loc, EN::Variant(sp(loc, (mident, access_name)), last_name))
                    }
                    result @ (ModuleIdent(_, _) | ModuleAccess(_, _) | Variant(_, _)) => {
                        ResolutionFailure(Box::new(result), InvalidKind("an address".to_string()))
                    }
                    result @ ResolutionFailure(_, _) => result,
                    UnresolvedName(_, _) => panic!("ICE failed in access chain expansion"),
                }
            }
        }
    }
}

impl PathExpander for Move2024PathExpander {
    fn push_alias_scope(&mut self, new_scope: AliasMapBuilder) {
        self.aliases.push_alias_scope(new_scope);
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
        use E::AttributeValue_ as EV;
        use P::{AttributeValue_ as PV, LeadingNameAccess_ as LN, NameAccessChain_ as PN};
        Some(sp(
            loc,
            match avalue_ {
                PV::Value(v) => EV::Value(value(context, v)?),
                PV::ModuleAccess(
                    sp!(ident_loc, PN::Two(sp!(aloc, LN::AnonymousAddress(a)), n)),
                ) => {
                    let addr = Address::anonymous(aloc, a);
                    let mident = sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n)));
                    if context.module_members.get(&mident).is_none() {
                        context.env.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (ident_loc, format!("Unbound module '{}'", mident))
                        ));
                    }
                    EV::Module(mident)
                }
                // TODO consider if we want to just force all of these checks into the well-known
                // attribute setup
                PV::ModuleAccess(access_chain) => {
                    match self.resolve_name_access_chain(context, access_chain) {
                        AccessChainResult::ModuleIdent(_, mident) => {
                            if context.module_members.get(&mident).is_none() {
                                context.env.add_diag(diag!(
                                    NameResolution::UnboundModule,
                                    (loc, format!("Unbound module '{}'", mident))
                                ));
                            }
                            EV::Module(mident)
                        }
                        AccessChainResult::ModuleAccess(loc, access) => {
                            EV::ModuleAccess(sp(loc, access))
                        }
                        AccessChainResult::Variant(loc, access) => {
                            EV::ModuleAccess(sp(loc, access))
                        }
                        AccessChainResult::UnresolvedName(loc, name) => {
                            EV::ModuleAccess(sp(loc, E::ModuleAccess_::Name(name)))
                        }
                        AccessChainResult::Address(loc, _) => {
                            let diag = diag!(
                                NameResolution::NamePositionMismatch,
                                (
                                    loc,
                                    "Found an address, but expected a module or module member"
                                        .to_string(),
                                )
                            );
                            context.env.add_diag(diag);
                            return None;
                        }
                        result @ AccessChainResult::ResolutionFailure(_, _) => {
                            context.env.add_diag(access_chain_resolution_error(result));
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
    ) -> Option<E::ModuleAccess> {
        use AccessChainResult::*;
        use E::ModuleAccess_ as EN;
        use P::NameAccessChain_ as PN;
        // This is a hack to let `use std::vector` play nicely with `vector`, plus preserve things like
        // `freeze`, etc.
        fn resolve_builtin_name(
            access: Access,
            chain: &P::NameAccessChain,
        ) -> Option<E::ModuleAccess> {
            match chain.value {
                PN::One(name) => match access {
                    Access::Type
                        if crate::naming::ast::BuiltinTypeName_::all_names()
                            .contains(&name.value) =>
                    {
                        Some(sp(name.loc, EN::Name(name)))
                    }
                    Access::ApplyPositional
                        if crate::naming::ast::BuiltinFunction_::all_names()
                            .contains(&name.value) =>
                    {
                        Some(sp(name.loc, EN::Name(name)))
                    }
                    _ => None,
                },
                _ => None,
            }
        }

        let loc = chain.loc;
        if let Some(builtin) = resolve_builtin_name(access, &chain) {
            Some(builtin)
        } else {
            let module_access = match access {
                Access::ApplyPositional | Access::ApplyNamed | Access::Type => {
                    let resolved_name = self.resolve_name_access_chain(context, chain.clone());
                    match resolved_name {
                        UnresolvedName(_, name) => EN::Name(name),
                        ModuleAccess(_, access) => access,
                        Variant(_, _) if matches!(access, Access::Type) => {
                            context.env.add_diag(unexpected_access_error(
                                resolved_name.loc(),
                                resolved_name.err_name(),
                                access,
                            ));
                            return None;
                        }
                        Variant(_, access) => access,
                        result @ Address(_, _) => {
                            context.env.add_diag(unexpected_access_error(
                                result.loc(),
                                result.err_name(),
                                access,
                            ));
                            return None;
                        }
                        result @ ModuleIdent(_, sp!(_, ModuleIdent_ { .. })) => {
                            let mut diag =
                                unexpected_access_error(result.loc(), result.err_name(), access);
                            if let ModuleIdent(_, sp!(_, ModuleIdent_ { address, module })) = result
                            {
                                let base_str = format!("{}", chain);
                                let realized_str = format!("{}::{}", address, module);
                                if base_str != realized_str {
                                    diag.add_note(format!(
                                        "Resolved '{}' to module identifier '{}'",
                                        base_str, realized_str
                                    ));
                                }
                                context.env.add_diag(diag);
                                return None;
                            } else {
                                unreachable!()
                            };
                        }
                        result @ ResolutionFailure(_, _) => {
                            context.env.add_diag(access_chain_resolution_error(result));
                            return None;
                        }
                    }
                }
                Access::Term => match chain.value {
                    PN::One(name)
                        if !is_valid_struct_constant_or_schema_name(&name.to_string()) =>
                    {
                        EN::Name(name)
                    }
                    _ => {
                        let resolved_name = self.resolve_name_access_chain(context, chain);
                        match resolved_name {
                            UnresolvedName(_, name) => EN::Name(name),
                            ModuleAccess(_, access) => access,
                            Variant(_, access) => access,
                            result @ (Address(_, _) | ModuleIdent(_, _)) => {
                                context.env.add_diag(unexpected_access_error(
                                    result.loc(),
                                    result.err_name(),
                                    Access::Term,
                                ));
                                return None;
                            }
                            result @ ResolutionFailure(_, _) => {
                                context.env.add_diag(access_chain_resolution_error(result));
                                return None;
                            }
                        }
                    }
                },
                Access::Variant => match chain.value {
                    PN::One(name) => EN::Name(name),
                    _ => {
                        let resolved_name = self.resolve_name_access_chain(context, chain);
                        match resolved_name {
                            UnresolvedName(_, name) => EN::Name(name),
                            Variant(_, access) => access,
                            result @ (Address(_, _) | ModuleIdent(_, _) | ModuleAccess(_, _)) => {
                                context.env.add_diag(unexpected_access_error(
                                    result.loc(),
                                    result.err_name(),
                                    Access::Variant,
                                ));
                                return None;
                            }
                            result @ ResolutionFailure(_, _) => {
                                context.env.add_diag(access_chain_resolution_error(result));
                                return None;
                            }
                        }
                    }
                },

                Access::Module => {
                    panic!("ICE module accesses can never resolve to a module member")
                }
            };
            Some(sp(loc, module_access))
        }
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        context: &mut DefnContext,
        chain: P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        use AccessChainResult::*;
        let resolved_name = self.resolve_name_access_chain(context, chain);
        match resolved_name {
            ModuleIdent(_, mident) => Some(mident),
            UnresolvedName(_, name) => {
                context.env.add_diag(unbound_module_error(name));
                None
            }
            result @ (Address(_, _) | ModuleAccess(_, _) | Variant(_, _)) => {
                context.env.add_diag(unexpected_access_error(
                    result.loc(),
                    result.err_name(),
                    Access::Module,
                ));
                None
            }
            result @ ResolutionFailure(_, _) => {
                context.env.add_diag(access_chain_resolution_error(result));
                None
            }
        }
    }
}

impl AccessChainResult {
    fn loc(&self) -> Loc {
        match self {
            AccessChainResult::Address(loc, _) => *loc,
            AccessChainResult::ModuleAccess(loc, _) => *loc,
            AccessChainResult::ModuleIdent(loc, _) => *loc,
            AccessChainResult::ResolutionFailure(inner, _) => inner.loc(),
            AccessChainResult::UnresolvedName(loc, _) => *loc,
            AccessChainResult::Variant(loc, _) => *loc,
        }
    }

    fn err_name(&self) -> String {
        match self {
            AccessChainResult::Address(_, _) => "address".to_string(),
            AccessChainResult::ModuleAccess(_, _) => "module member".to_string(),
            AccessChainResult::ModuleIdent(_, _) => "module".to_string(),
            AccessChainResult::ResolutionFailure(inner, _) => inner.err_name(),
            AccessChainResult::UnresolvedName(_, _) => "name".to_string(),
            AccessChainResult::Variant(_, _) => "variant".to_string(),
        }
    }
}

fn unexpected_access_error(loc: Loc, result: String, access: Access) -> Diagnostic {
    let case = match access {
        Access::Type | Access::ApplyNamed => "type",
        Access::ApplyPositional => "expression",
        Access::Term => "expression",
        Access::Variant => "variant",
        Access::Module => "module",
    };
    let unexpected_msg = if result.starts_with('a') {
        format!(
            "Unexpected {0}. An {0} identifier is not a valid {1}",
            result, case
        )
    } else {
        format!(
            "Unexpected {0}. A {0} identifier is not a valid {1}",
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

fn access_chain_resolution_error(result: AccessChainResult) -> Diagnostic {
    if let AccessChainResult::ResolutionFailure(inner, reason) = result {
        let loc = inner.loc();
        let msg = match reason {
            AccessChainFailure::InvalidKind(kind) => format!(
                "Expected {} in this position, not a {}",
                kind,
                inner.err_name()
            ),
            AccessChainFailure::UnresolvedAlias(name) => {
                format!("Could not resolve the name '{}'", name)
            }
        };
        diag!(NameResolution::NamePositionMismatch, (loc, msg))
    } else {
        panic!("ICE miscalled resolution error handler")
    }
}

//**************************************************************************************************
// Aliases
//**************************************************************************************************

fn all_module_members<'a>(
    context: &mut DefnContext,
    named_addr_maps: &NamedAddressMaps,
    members: &mut UniqueMap<ModuleIdent, ModuleMembers>,
    always_add: bool,
    defs: impl IntoIterator<Item = &'a P::PackageDefinition>,
) {
    for P::PackageDefinition {
        named_address_map: named_address_map_index,
        def,
        ..
    } in defs
    {
        let named_addr_map: &NamedAddressMap = named_addr_maps.get(*named_address_map_index);
        match def {
            P::Definition::Module(m) => {
                let addr = match &m.address {
                    Some(a) => top_level_address_(
                        context,
                        named_addr_map,
                        /* suggest_declaration */ true,
                        *a,
                    ),
                    // Error will be handled when the module is compiled
                    None => Address::anonymous(m.loc, NumericalAddress::DEFAULT_ERROR_ADDRESS),
                };
                module_members(members, always_add, addr, m)
            }
            P::Definition::Address(addr_def) => {
                let addr = top_level_address_(
                    context,
                    named_addr_map,
                    /* suggest_declaration */ false,
                    addr_def.addr,
                );
                for m in &addr_def.modules {
                    module_members(members, always_add, addr, m)
                }
            }
        };
    }
}

fn module_members(
    members: &mut UniqueMap<ModuleIdent, ModuleMembers>,
    always_add: bool,
    address: Address,
    m: &P::ModuleDefinition,
) {
    let mident = sp(m.name.loc(), ModuleIdent_::new(address, m.name));
    if !always_add && members.contains_key(&mident) {
        return;
    }
    let mut cur_members = members.remove(&mident).unwrap_or_default();
    for mem in &m.members {
        use P::{SpecBlockMember_ as SBM, SpecBlockTarget_ as SBT, SpecBlock_ as SB};
        match mem {
            P::ModuleMember::Function(f) => {
                cur_members.insert(f.name.0, ModuleMemberKind::Function);
            }
            P::ModuleMember::Constant(c) => {
                cur_members.insert(c.name.0, ModuleMemberKind::Constant);
            }
            P::ModuleMember::Struct(s) => {
                cur_members.insert(s.name.0, ModuleMemberKind::Struct);
            }
            P::ModuleMember::Enum(e) => {
                cur_members.insert(e.name.0, ModuleMemberKind::Enum);
            }
            P::ModuleMember::Spec(
                sp!(
                    _,
                    SB {
                        target,
                        members,
                        ..
                    }
                ),
            ) => match &target.value {
                SBT::Schema(n, _) => {
                    cur_members.insert(*n, ModuleMemberKind::Schema);
                }
                SBT::Module => {
                    for sp!(_, smember_) in members {
                        if let SBM::Function { name, .. } = smember_ {
                            cur_members.insert(name.0, ModuleMemberKind::Function);
                        }
                    }
                }
                _ => (),
            },
            P::ModuleMember::Use(_) | P::ModuleMember::Friend(_) => (),
        };
    }
    members.add(mident, cur_members).unwrap();
}

fn named_addr_map_to_alias_map_builder(
    context: &mut Context,
    named_addr_map: &NamedAddressMap,
) -> AliasMapBuilder {
    let mut new_aliases = context.new_alias_map_builder();
    for (name, addr) in named_addr_map {
        // Address symbols get dummy locations so that we can lift them to names. These should
        // always be rewritten with more-accurate information as they are used.
        new_aliases
            .add_address_alias(sp(Loc::invalid(), *name), *addr)
            .expect("ICE dupe address");
    }
    new_aliases
}

fn module_self_aliases(acc: &mut AliasMapBuilder, current_module: &ModuleIdent) {
    let self_name = sp(current_module.loc, ModuleName::SELF_NAME.into());
    acc.add_implicit_module_alias(self_name, *current_module)
        .unwrap()
}

fn aliases_from_member(
    context: &mut Context,
    acc: &mut AliasMapBuilder,
    use_funs: &mut UseFunsBuilder,
    current_module: &ModuleIdent,
    member: P::ModuleMember,
) -> Option<P::ModuleMember> {
    use P::{SpecBlockMember_ as SBM, SpecBlockTarget_ as SBT, SpecBlock_ as SB};
    macro_rules! check_name_and_add_implicit_alias {
        ($kind:expr, $name:expr) => {{
            if let Some(n) = check_valid_module_member_name(context, $kind, $name) {
                if let Err(loc) =
                    acc.add_implicit_member_alias(n.clone(), current_module.clone(), n.clone())
                {
                    duplicate_module_member(context, loc, n)
                }
            }
        }};
    }

    match member {
        P::ModuleMember::Use(u) => {
            use_(context, acc, use_funs, u);
            None
        }
        f @ P::ModuleMember::Friend(_) => {
            // friend declarations do not produce implicit aliases
            Some(f)
        }
        P::ModuleMember::Function(f) => {
            let n = f.name.0;
            check_name_and_add_implicit_alias!(ModuleMemberKind::Function, n);
            Some(P::ModuleMember::Function(f))
        }
        P::ModuleMember::Constant(c) => {
            let n = c.name.0;
            check_name_and_add_implicit_alias!(ModuleMemberKind::Constant, n);
            Some(P::ModuleMember::Constant(c))
        }
        P::ModuleMember::Struct(s) => {
            let n = s.name.0;
            check_name_and_add_implicit_alias!(ModuleMemberKind::Struct, n);
            Some(P::ModuleMember::Struct(s))
        }
        P::ModuleMember::Enum(e) => {
            let n = e.name.0;
            check_name_and_add_implicit_alias!(ModuleMemberKind::Enum, n);
            Some(P::ModuleMember::Enum(e))
        }
        P::ModuleMember::Spec(s) => {
            let sp!(
                _,
                SB {
                    target,
                    members,
                    ..
                }
            ) = &s;
            match &target.value {
                SBT::Schema(n, _) => {
                    check_name_and_add_implicit_alias!(ModuleMemberKind::Schema, *n);
                }
                SBT::Module => {
                    for sp!(_, smember_) in members {
                        if let SBM::Function { name, .. } = smember_ {
                            let n = name.0;
                            check_name_and_add_implicit_alias!(ModuleMemberKind::Function, n);
                        }
                    }
                }
                _ => (),
            };
            Some(P::ModuleMember::Spec(s))
        }
    }
}

fn uses(context: &mut Context, uses: Vec<P::UseDecl>) -> (AliasMapBuilder, UseFunsBuilder) {
    let mut new_scope = context.new_alias_map_builder();
    let mut use_funs = UseFunsBuilder::new();
    for u in uses {
        use_(context, &mut new_scope, &mut use_funs, u);
    }
    (new_scope, use_funs)
}

fn use_(
    context: &mut Context,
    acc: &mut AliasMapBuilder,
    use_funs: &mut UseFunsBuilder,
    u: P::UseDecl,
) {
    let P::UseDecl {
        use_: u,
        loc,
        attributes,
    } = u;
    let attributes = flatten_attributes(context, AttributePosition::Use, attributes);
    match u {
        P::Use::NestedModuleUses(address, use_decls) => {
            for (module, use_) in use_decls {
                let mident = sp(module.loc(), P::ModuleIdent_ { address, module });
                module_use(context, acc, use_funs, mident, &attributes, use_);
            }
        }
        P::Use::ModuleUse(mident, use_) => {
            module_use(context, acc, use_funs, mident, &attributes, use_);
        }
        P::Use::Fun {
            visibility,
            function,
            ty,
            method,
        } => {
            let pkg = context.current_package;
            context.env().check_feature(FeatureGate::DotCall, pkg, loc);
            let is_public = match visibility {
                P::Visibility::Public(vis_loc) => Some(vis_loc),
                P::Visibility::Internal => None,
                P::Visibility::Friend(vis_loc) | P::Visibility::Package(vis_loc) => {
                    let msg = "Invalid visibility for 'use fun' declaration";
                    let vis_msg = format!(
                        "Module level 'use fun' declarations can be '{}' for the module's types, \
                    otherwise they must internal to declared scope.",
                        P::Visibility::PUBLIC
                    );
                    context.env().add_diag(diag!(
                        Declarations::InvalidUseFun,
                        (loc, msg),
                        (vis_loc, vis_msg)
                    ));
                    None
                }
            };
            let explicit = ParserExplicitUseFun {
                loc,
                attributes,
                is_public,
                function,
                ty,
                method,
            };
            use_funs.explicit.push(explicit);
        }
    }
}

fn module_use(
    context: &mut Context,
    acc: &mut AliasMapBuilder,
    use_funs: &mut UseFunsBuilder,
    in_mident: P::ModuleIdent,
    attributes: &E::Attributes,
    muse: P::ModuleUse,
) {
    let unbound_module = |mident: &ModuleIdent| -> Diagnostic {
        diag!(
            NameResolution::UnboundModule,
            (
                mident.loc,
                format!("Invalid 'use'. Unbound module: '{}'", mident),
            )
        )
    };
    macro_rules! add_module_alias {
        ($ident:expr, $alias:expr) => {{
            if let Err(()) = check_restricted_name_all_cases(
                &mut context.defn_context,
                NameCase::ModuleAlias,
                &$alias,
            ) {
                return;
            }

            if let Err(old_loc) = acc.add_module_alias($alias.clone(), $ident) {
                duplicate_module_alias(context, old_loc, $alias)
            }
        }};
    }
    match muse {
        P::ModuleUse::Module(alias_opt) => {
            let mident = module_ident(&mut context.defn_context, in_mident);
            if !context.defn_context.module_members.contains_key(&mident) {
                context.env().add_diag(unbound_module(&mident));
                return;
            };
            let alias = alias_opt
                .map(|m| m.0)
                .unwrap_or_else(|| mident.value.module.0);
            add_module_alias!(mident, alias)
        }
        P::ModuleUse::Members(sub_uses) => {
            let mident = module_ident(&mut context.defn_context, in_mident);
            let members = match context.defn_context.module_members.get(&mident) {
                Some(members) => members,
                None => {
                    context.env().add_diag(unbound_module(&mident));
                    return;
                }
            };
            let mloc = *context
                .defn_context
                .module_members
                .get_loc(&mident)
                .unwrap();
            let sub_uses_kinds = sub_uses
                .into_iter()
                .map(|(member, alia_opt)| {
                    let kind = members.get(&member).cloned();
                    (member, alia_opt, kind)
                })
                .collect::<Vec<_>>();

            for (member, alias_opt, member_kind_opt) in sub_uses_kinds {
                if member.value.as_str() == ModuleName::SELF_NAME {
                    let alias = if let Some(alias) = alias_opt {
                        alias
                    } else {
                        // For Self-inclusion, we respan the symbol to point to Self for better
                        // error messages.
                        let symbol = mident.value.module.0.value;
                        sp(member.loc, symbol)
                    };
                    add_module_alias!(mident, alias);
                    continue;
                }

                // check is member

                let member_kind = match member_kind_opt {
                    None => {
                        let msg = format!(
                            "Invalid 'use'. Unbound member '{}' in module '{}'",
                            member, mident
                        );
                        context.env().add_diag(diag!(
                            NameResolution::UnboundModuleMember,
                            (member.loc, msg),
                            (mloc, format!("Module '{}' declared here", mident)),
                        ));
                        continue;
                    }
                    Some(m) => m,
                };

                let alias = alias_opt.unwrap_or(member);

                let alias = match check_valid_module_member_alias(context, member_kind, alias) {
                    None => continue,
                    Some(alias) => alias,
                };
                if let Err(old_loc) = acc.add_member_alias(alias, mident, member) {
                    duplicate_module_member(context, old_loc, alias)
                }
                if matches!(member_kind, ModuleMemberKind::Function) {
                    // remove any previously declared alias to keep in sync with the member alias
                    // map
                    use_funs.implicit.remove(&alias);
                    // not a function declaration
                    let is_public = None;
                    // assume used. We will set it to false if needed when exiting this alias scope
                    let kind = E::ImplicitUseFunKind::UseAlias { used: true };
                    let implicit = E::ImplicitUseFunCandidate {
                        loc: alias.loc,
                        attributes: attributes.clone(),
                        is_public,
                        function: (mident, member),
                        kind,
                    };
                    use_funs.implicit.add(alias, implicit).unwrap();
                }
            }
        }
    }
}

fn use_funs(context: &mut Context, builder: UseFunsBuilder) -> E::UseFuns {
    let UseFunsBuilder {
        explicit: pexplicit,
        implicit,
    } = builder;
    // If None, there was an error and we can skip it
    let explicit = pexplicit
        .into_iter()
        .filter_map(|e| explicit_use_fun(context, e))
        .collect();
    E::UseFuns { explicit, implicit }
}

fn explicit_use_fun(
    context: &mut Context,
    pexplicit: ParserExplicitUseFun,
) -> Option<E::ExplicitUseFun> {
    let ParserExplicitUseFun {
        loc,
        attributes,
        is_public,
        function,
        ty,
        method,
    } = pexplicit;
    let function =
        context.name_access_chain_to_module_access(Access::ApplyPositional, *function)?;
    let ty = context.name_access_chain_to_module_access(Access::Type, *ty)?;
    Some(E::ExplicitUseFun {
        loc,
        attributes,
        is_public,
        function,
        ty,
        method,
    })
}

fn duplicate_module_alias(context: &mut Context, old_loc: Loc, alias: Name) {
    let msg = format!(
        "Duplicate module alias '{}'. Module aliases must be unique within a given namespace",
        alias
    );
    context.env().add_diag(diag!(
        Declarations::DuplicateItem,
        (alias.loc, msg),
        (old_loc, "Alias previously defined here"),
    ));
}

fn duplicate_module_member(context: &mut Context, old_loc: Loc, alias: Name) {
    let msg = format!(
        "Duplicate module member or alias '{}'. Top level names in a namespace must be unique",
        alias
    );
    context.env().add_diag(diag!(
        Declarations::DuplicateItem,
        (alias.loc, msg),
        (old_loc, "Alias previously defined here"),
    ));
}

fn unused_alias(context: &mut Context, _kind: &str, alias: Name) {
    if !context.is_source_definition {
        return;
    }
    let mut diag = diag!(
        UnusedItem::Alias,
        (
            alias.loc,
            format!("Unused 'use' of alias '{}'. Consider removing it", alias)
        ),
    );
    if crate::naming::ast::BuiltinTypeName_::all_names().contains(&alias.value) {
        diag.add_note(format!(
            "This alias does not shadow the built-in type '{}' in type annotations.",
            alias
        ));
    } else if crate::naming::ast::BuiltinFunction_::all_names().contains(&alias.value) {
        diag.add_note(format!(
            "This alias does not shadow the built-in function '{}' in call expressions.",
            alias
        ));
    }
    context.env().add_diag(diag);
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn struct_def(
    context: &mut Context,
    structs: &mut UniqueMap<DatatypeName, E::StructDefinition>,
    pstruct: P::StructDefinition,
) {
    let (sname, sdef) = struct_def_(context, structs.len(), pstruct);
    if let Err(_old_loc) = structs.add(sname, sdef) {
        assert!(context.env().has_errors())
    }
}

fn struct_def_(
    context: &mut Context,
    index: usize,
    pstruct: P::StructDefinition,
) -> (DatatypeName, E::StructDefinition) {
    let P::StructDefinition {
        attributes,
        loc,
        name,
        abilities: abilities_vec,
        type_parameters: pty_params,
        fields: pfields,
    } = pstruct;
    let attributes = flatten_attributes(context, AttributePosition::Struct, attributes);
    let warning_filter = warning_filter(context, &attributes);
    context
        .env()
        .add_warning_filter_scope(warning_filter.clone());
    let type_parameters = struct_type_parameters(context, pty_params);
    context.push_type_parameters(type_parameters.iter().map(|tp| &tp.name));
    let abilities = ability_set(context, "modifier", abilities_vec);
    let fields = struct_fields(context, &name, pfields);
    let sdef = E::StructDefinition {
        warning_filter,
        index,
        attributes,
        loc,
        abilities,
        type_parameters,
        fields,
    };
    context.pop_alias_scope(None);
    context.env().pop_warning_filter_scope();
    (name, sdef)
}

fn struct_fields(
    context: &mut Context,
    sname: &DatatypeName,
    pfields: P::StructFields,
) -> E::StructFields {
    let pfields_vec = match pfields {
        P::StructFields::Native(loc) => return E::StructFields::Native(loc),
        P::StructFields::Positional(tys) => {
            let field_tys = tys.into_iter().map(|fty| type_(context, fty)).collect();
            return E::StructFields::Positional(field_tys);
        }
        P::StructFields::Named(v) => v,
    };
    let mut field_map = UniqueMap::new();
    for (idx, (field, pt)) in pfields_vec.into_iter().enumerate() {
        let t = type_(context, pt);
        if let Err((field, old_loc)) = field_map.add(field, (idx, t)) {
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (
                    field.loc(),
                    format!(
                        "Duplicate definition for field '{}' in struct '{}'",
                        field, sname
                    ),
                ),
                (old_loc, "Field previously defined here"),
            ));
        }
    }
    E::StructFields::Named(field_map)
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

fn enum_def(
    context: &mut Context,
    enums: &mut UniqueMap<DatatypeName, E::EnumDefinition>,
    penum: P::EnumDefinition,
) {
    let (ename, edef) = enum_def_(context, enums.len(), penum);
    if let Err(_old_loc) = enums.add(ename, edef) {
        assert!(context.env().has_errors())
    }
}

fn enum_def_(
    context: &mut Context,
    index: usize,
    penum: P::EnumDefinition,
) -> (DatatypeName, E::EnumDefinition) {
    let P::EnumDefinition {
        attributes,
        loc,
        name,
        abilities: abilities_vec,
        type_parameters: pty_params,
        variants: pvariants,
    } = penum;
    let attributes = flatten_attributes(context, AttributePosition::Enum, attributes);
    let warning_filter = warning_filter(context, &attributes);
    context
        .env()
        .add_warning_filter_scope(warning_filter.clone());
    let type_parameters = enum_type_parameters(context, pty_params);
    context.push_type_parameters(type_parameters.iter().map(|tp| &tp.name));
    let abilities = ability_set(context, "modifier", abilities_vec);
    let variants = enum_variants(context, &name, pvariants);
    let edef = E::EnumDefinition {
        warning_filter,
        index,
        attributes,
        loc,
        abilities,
        type_parameters,
        variants,
    };
    context.pop_alias_scope(None);
    context.env().pop_warning_filter_scope();
    (name, edef)
}

fn enum_variants(
    context: &mut Context,
    ename: &DatatypeName,
    pvariants: Vec<P::VariantDefinition>,
) -> UniqueMap<VariantName, E::VariantDefinition> {
    let mut variants = UniqueMap::new();
    for variant in pvariants {
        let loc = variant.loc;
        let (vname, vdef) = enum_variant_def(context, variants.len(), variant);
        if let Err(old_loc) = variants.add(vname, vdef) {
            let msg: String = format!(
                "Duplicate definition for variant '{}' in enum '{}'",
                vname, ename
            );
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, msg),
                (old_loc.1, "Variant previously defined here")
            ));
        }
    }
    variants
}

fn enum_variant_def(
    context: &mut Context,
    index: usize,
    pvariant: P::VariantDefinition,
) -> (VariantName, E::VariantDefinition) {
    let P::VariantDefinition { loc, name, fields } = pvariant;
    let fields = variant_fields(context, &name, fields);
    let vdef = E::VariantDefinition { loc, index, fields };
    (name, vdef)
}

fn variant_fields(
    context: &mut Context,
    vname: &VariantName,
    pfields: P::VariantFields,
) -> E::VariantFields {
    let pfields_vec = match pfields {
        P::VariantFields::Empty => return E::VariantFields::Empty,
        P::VariantFields::Positional(tys) => {
            let field_tys = tys.into_iter().map(|fty| type_(context, fty)).collect();
            return E::VariantFields::Positional(field_tys);
        }
        P::VariantFields::Named(v) => v,
    };
    let mut field_map = UniqueMap::new();
    for (idx, (field, pt)) in pfields_vec.into_iter().enumerate() {
        let t = type_(context, pt);
        if let Err((field, old_loc)) = field_map.add(field, (idx, t)) {
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (
                    field.loc(),
                    format!(
                        "Duplicate definition for field '{}' in variant '{}'",
                        field, vname
                    ),
                ),
                (old_loc, "Field previously defined here"),
            ));
        }
    }
    E::VariantFields::Named(field_map)
}

//**************************************************************************************************
// Friends
//**************************************************************************************************

fn friend(
    context: &mut Context,
    friends: &mut UniqueMap<ModuleIdent, E::Friend>,
    pfriend: P::FriendDecl,
) {
    match friend_(context, pfriend) {
        Some((mident, friend)) => match friends.get(&mident) {
            None => friends.add(mident, friend).unwrap(),
            Some(old_friend) => {
                let msg = format!(
                    "Duplicate friend declaration '{}'. Friend declarations in a module must be \
                     unique",
                    mident
                );
                context.env().add_diag(diag!(
                    Declarations::DuplicateItem,
                    (friend.loc, msg),
                    (old_friend.loc, "Friend previously declared here"),
                ));
            }
        },
        None => assert!(context.env().has_errors()),
    };
}

fn friend_(context: &mut Context, pfriend_decl: P::FriendDecl) -> Option<(ModuleIdent, E::Friend)> {
    let P::FriendDecl {
        attributes: pattributes,
        loc,
        friend: pfriend,
    } = pfriend_decl;
    let mident = context.name_access_chain_to_module_ident(pfriend)?;
    let attributes = flatten_attributes(context, AttributePosition::Friend, pattributes);
    Some((mident, E::Friend { attributes, loc }))
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(
    context: &mut Context,
    constants: &mut UniqueMap<ConstantName, E::Constant>,
    pconstant: P::Constant,
) {
    let (name, constant) = constant_(context, constants.len(), pconstant);
    if let Err(_old_loc) = constants.add(name, constant) {
        assert!(context.env().has_errors())
    }
}

fn constant_(
    context: &mut Context,
    index: usize,
    pconstant: P::Constant,
) -> (ConstantName, E::Constant) {
    let P::Constant {
        attributes: pattributes,
        loc,
        name,
        signature: psignature,
        value: pvalue,
    } = pconstant;
    let attributes = flatten_attributes(context, AttributePosition::Constant, pattributes);
    let warning_filter = warning_filter(context, &attributes);
    context
        .env()
        .add_warning_filter_scope(warning_filter.clone());
    let signature = type_(context, psignature);
    let value = exp_(context, pvalue);
    let constant = E::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value,
    };
    context.env().pop_warning_filter_scope();
    (name, constant)
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(
    context: &mut Context,
    module_and_use_funs: Option<(ModuleIdent, &mut UseFunsBuilder)>,
    functions: &mut UniqueMap<FunctionName, E::Function>,
    pfunction: P::Function,
) {
    let (fname, fdef) = function_(context, module_and_use_funs, functions.len(), pfunction);
    if let Err(_old_loc) = functions.add(fname, fdef) {
        assert!(context.env().has_errors())
    }
}

fn function_(
    context: &mut Context,
    module_and_use_funs: Option<(ModuleIdent, &mut UseFunsBuilder)>,
    index: usize,
    pfunction: P::Function,
) -> (FunctionName, E::Function) {
    let P::Function {
        attributes: pattributes,
        loc,
        name,
        visibility: pvisibility,
        entry,
        signature: psignature,
        body: pbody,
    } = pfunction;
    let attributes = flatten_attributes(context, AttributePosition::Function, pattributes);
    let warning_filter = warning_filter(context, &attributes);
    context
        .env()
        .add_warning_filter_scope(warning_filter.clone());
    let visibility = visibility(pvisibility);
    let signature = function_signature(context, psignature);
    let body = function_body(context, pbody);
    if let Some((m, use_funs_builder)) = module_and_use_funs {
        let implicit = E::ImplicitUseFunCandidate {
            loc: name.loc(),
            attributes: attributes.clone(),
            is_public: Some(visibility.loc().unwrap_or_else(|| name.loc())),
            function: (m, name.0),
            // disregard used/unused information tracking
            kind: E::ImplicitUseFunKind::FunctionDeclaration,
        };
        // we can ignore any error, since the alias map will catch conflicting names
        let _ = use_funs_builder.implicit.add(name.0, implicit);
    }
    let fdef = E::Function {
        warning_filter,
        index,
        attributes,
        loc,
        visibility,
        entry,
        signature,
        body,
    };
    context.pop_alias_scope(None);
    context.env().pop_warning_filter_scope();
    (name, fdef)
}

fn visibility(pvisibility: P::Visibility) -> E::Visibility {
    match pvisibility {
        P::Visibility::Friend(loc) => E::Visibility::Friend(loc),
        P::Visibility::Internal => E::Visibility::Internal,
        P::Visibility::Package(loc) => E::Visibility::Package(loc),
        P::Visibility::Public(loc) => E::Visibility::Public(loc),
    }
}

fn function_signature(
    context: &mut Context,
    psignature: P::FunctionSignature,
) -> E::FunctionSignature {
    let P::FunctionSignature {
        type_parameters: pty_params,
        parameters: pparams,
        return_type: pret_ty,
    } = psignature;
    let type_parameters = type_parameters(context, pty_params);
    context.push_type_parameters(type_parameters.iter().map(|(name, _)| name));
    let parameters = pparams
        .into_iter()
        .map(|(pmut, v, t)| (mutability(context, v.loc(), pmut), v, type_(context, t)))
        .collect::<Vec<_>>();
    for (_, v, _) in &parameters {
        check_valid_local_name(context, v)
    }
    let return_type = type_(context, pret_ty);
    E::FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    }
}

fn function_body(context: &mut Context, sp!(loc, pbody_): P::FunctionBody) -> E::FunctionBody {
    use E::FunctionBody_ as EF;
    use P::FunctionBody_ as PF;
    let body_ = match pbody_ {
        PF::Native => EF::Native,
        PF::Defined(seq) => EF::Defined(sequence(context, loc, seq)),
    };
    sp(loc, body_)
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn ability_set(context: &mut Context, case: &str, abilities_vec: Vec<Ability>) -> E::AbilitySet {
    let mut set = E::AbilitySet::empty();
    for ability in abilities_vec {
        let loc = ability.loc;
        if let Err(prev_loc) = set.add(ability) {
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, format!("Duplicate '{}' ability {}", ability, case)),
                (prev_loc, "Ability previously given here")
            ));
        }
    }
    set
}

fn type_parameters(
    context: &mut Context,
    pty_params: Vec<(Name, Vec<Ability>)>,
) -> Vec<(Name, E::AbilitySet)> {
    pty_params
        .into_iter()
        .map(|(name, constraints_vec)| {
            let constraints = ability_set(context, "constraint", constraints_vec);
            (name, constraints)
        })
        .collect()
}

fn struct_type_parameters(
    context: &mut Context,
    pty_params: Vec<P::DatatypeTypeParameter>,
) -> Vec<E::DatatypeTypeParameter> {
    pty_params
        .into_iter()
        .map(|param| E::DatatypeTypeParameter {
            is_phantom: param.is_phantom,
            name: param.name,
            constraints: ability_set(context, "constraint", param.constraints),
        })
        .collect()
}

fn enum_type_parameters(
    context: &mut Context,
    pty_params: Vec<P::DatatypeTypeParameter>,
) -> Vec<E::DatatypeTypeParameter> {
    pty_params
        .into_iter()
        .map(|param| E::DatatypeTypeParameter {
            is_phantom: param.is_phantom,
            name: param.name,
            constraints: ability_set(context, "constraint", param.constraints),
        })
        .collect()
}

fn type_(context: &mut Context, sp!(loc, pt_): P::Type) -> E::Type {
    use E::Type_ as ET;
    use P::Type_ as PT;
    let t_ = match pt_ {
        PT::Unit => ET::Unit,
        PT::Multiple(ts) => ET::Multiple(types(context, ts)),
        PT::Apply(pn, ptyargs) => {
            let tyargs = types(context, ptyargs);
            match context.name_access_chain_to_module_access(Access::Type, *pn) {
                None => {
                    assert!(context.env().has_errors());
                    ET::UnresolvedError
                }
                Some(n) => ET::Apply(n, tyargs),
            }
        }
        PT::Ref(mut_, inner) => ET::Ref(mut_, Box::new(type_(context, *inner))),
        PT::Fun(_, _) => {
            // TODO these will be used later by macros
            context.spec_deprecated(loc, /* is_error */ true);
            ET::UnresolvedError
        }
    };
    sp(loc, t_)
}

fn types(context: &mut Context, pts: Vec<P::Type>) -> Vec<E::Type> {
    pts.into_iter().map(|pt| type_(context, pt)).collect()
}

fn optional_types(context: &mut Context, pts_opt: Option<Vec<P::Type>>) -> Option<Vec<E::Type>> {
    pts_opt.map(|pts| pts.into_iter().map(|pt| type_(context, pt)).collect())
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

fn sequence(context: &mut Context, loc: Loc, seq: P::Sequence) -> E::Sequence {
    let (puses, pitems, maybe_last_semicolon_loc, pfinal_item) = seq;

    let (new_scope, use_funs_builder) = uses(context, puses);
    context.push_alias_scope(new_scope);
    let mut use_funs = use_funs(context, use_funs_builder);
    let mut items: VecDeque<E::SequenceItem> = pitems
        .into_iter()
        .map(|item| sequence_item(context, item))
        .collect();
    let final_e_opt = pfinal_item.map(|item| exp_(context, item));
    let final_e = match final_e_opt {
        None => {
            let last_semicolon_loc = match maybe_last_semicolon_loc {
                Some(l) => l,
                None => loc,
            };
            sp(last_semicolon_loc, E::Exp_::Unit { trailing: true })
        }
        Some(e) => e,
    };
    let final_item = sp(final_e.loc, E::SequenceItem_::Seq(final_e));
    items.push_back(final_item);
    context.pop_alias_scope(Some(&mut use_funs));
    (use_funs, items)
}

fn sequence_item(context: &mut Context, sp!(loc, pitem_): P::SequenceItem) -> E::SequenceItem {
    use E::SequenceItem_ as ES;
    use P::SequenceItem_ as PS;
    let item_ = match pitem_ {
        PS::Seq(e) => ES::Seq(exp_(context, *e)),
        PS::Declare(pb, pty_opt) => {
            let b_opt = bind_list(context, pb);
            let ty_opt = pty_opt.map(|t| type_(context, t));
            match b_opt {
                None => {
                    assert!(context.env().has_errors());
                    ES::Seq(sp(loc, E::Exp_::UnresolvedError))
                }
                Some(b) => ES::Declare(b, ty_opt),
            }
        }
        PS::Bind(pb, pty_opt, pe) => {
            let b_opt = bind_list(context, pb);
            let ty_opt = pty_opt.map(|t| type_(context, t));
            let e_ = exp_(context, *pe);
            let e = match ty_opt {
                None => e_,
                Some(ty) => sp(e_.loc, E::Exp_::Annotate(Box::new(e_), ty)),
            };
            match b_opt {
                None => {
                    assert!(context.env().has_errors());
                    ES::Seq(sp(loc, E::Exp_::UnresolvedError))
                }
                Some(b) => ES::Bind(b, e),
            }
        }
    };
    sp(loc, item_)
}

fn exps(context: &mut Context, pes: Vec<P::Exp>) -> Vec<E::Exp> {
    pes.into_iter().map(|pe| exp_(context, pe)).collect()
}

fn exp(context: &mut Context, pe: P::Exp) -> Box<E::Exp> {
    Box::new(exp_(context, pe))
}

fn exp_(context: &mut Context, sp!(loc, pe_): P::Exp) -> E::Exp {
    use E::Exp_ as EE;
    use P::Exp_ as PE;
    let e_ = match pe_ {
        PE::Unit => EE::Unit { trailing: false },
        PE::Value(pv) => match value(&mut context.defn_context, pv) {
            Some(v) => EE::Value(v),
            None => {
                assert!(context.env().has_errors());
                EE::UnresolvedError
            }
        },
        PE::Name(_, Some(_)) => {
            let msg = "Expected name to be followed by a brace-enclosed list of field expressions \
                or a parenthesized list of arguments for a function call";
            context
                .env()
                .add_diag(diag!(NameResolution::NamePositionMismatch, (loc, msg)));
            EE::UnresolvedError
        }
        PE::Name(pn, ptys_opt) => {
            let en_opt = context.name_access_chain_to_module_access(Access::Term, pn);
            let tys_opt = optional_types(context, ptys_opt);
            match en_opt {
                Some(en) => EE::Name(en, tys_opt),
                None => {
                    assert!(context.env().has_errors());
                    EE::UnresolvedError
                }
            }
        }
        PE::Call(pn, is_macro, ptys_opt, sp!(rloc, prs)) => {
            let tys_opt = optional_types(context, ptys_opt);
            let ers = sp(rloc, exps(context, prs));
            let en_opt = context.name_access_chain_to_module_access(Access::ApplyPositional, pn);
            match en_opt {
                Some(en) => EE::Call(en, is_macro, tys_opt, ers),
                None => {
                    assert!(context.env().has_errors());
                    EE::UnresolvedError
                }
            }
        }
        PE::Pack(pn, ptys_opt, pfields) => {
            let en_opt = context.name_access_chain_to_module_access(Access::ApplyNamed, pn);
            let tys_opt = optional_types(context, ptys_opt);
            let efields_vec = pfields
                .into_iter()
                .map(|(f, pe)| (f, exp_(context, pe)))
                .collect();
            let efields = named_fields(context, loc, "construction", "argument", efields_vec);
            match en_opt {
                Some(en) => EE::Pack(en, tys_opt, efields),
                None => {
                    assert!(context.env().has_errors());
                    EE::UnresolvedError
                }
            }
        }
        PE::Vector(vec_loc, ptys_opt, sp!(args_loc, pargs_)) => {
            let tys_opt = optional_types(context, ptys_opt);
            let args = sp(args_loc, exps(context, pargs_));
            EE::Vector(vec_loc, tys_opt, args)
        }
        PE::IfElse(pb, pt, pf_opt) => {
            let eb = exp(context, *pb);
            let et = exp(context, *pt);
            let ef = match pf_opt {
                None => Box::new(sp(loc, EE::Unit { trailing: false })),
                Some(pf) => exp(context, *pf),
            };
            EE::IfElse(eb, et, ef)
        }
        PE::Match(subject, sp!(aloc, arms)) => EE::Match(
            exp(context, *subject),
            sp(
                aloc,
                arms.into_iter()
                    .map(|arm| match_arm(context, arm))
                    .collect(),
            ),
        ),
        PE::While(pb, ploop) => {
            let (name, body) = maybe_named_loop(context, loc, *ploop);
            EE::While(exp(context, *pb), name, body)
        }
        PE::Loop(ploop) => {
            let (name, body) = maybe_named_loop(context, loc, *ploop);
            EE::Loop(name, body)
        }
        PE::NamedBlock(name, seq) => EE::NamedBlock(name, sequence(context, loc, seq)),
        PE::Block(seq) => EE::Block(sequence(context, loc, seq)),
        PE::Lambda(..) | PE::Quant(..) => {
            // TODO lambdas will be used later by macros
            context.spec_deprecated(loc, /* is_error */ true);
            EE::UnresolvedError
        }
        PE::ExpList(pes) => {
            assert!(pes.len() > 1);
            EE::ExpList(exps(context, pes))
        }

        PE::Assign(lvalue, rhs) => {
            let l_opt = lvalues(context, *lvalue);
            let er = exp(context, *rhs);
            match l_opt {
                None => {
                    assert!(context.env().has_errors());
                    EE::UnresolvedError
                }
                Some(LValue::Assigns(al)) => EE::Assign(al, er),
                Some(LValue::Mutate(el)) => EE::Mutate(el, er),
                Some(LValue::FieldMutate(edotted)) => EE::FieldMutate(edotted, er),
            }
        }
        PE::Abort(pe) => EE::Abort(exp(context, *pe)),
        PE::Return(name_opt, pe_opt) => {
            let ev = match pe_opt {
                None => Box::new(sp(loc, EE::Unit { trailing: false })),
                Some(pe) => exp(context, *pe),
            };
            EE::Return(name_opt, ev)
        }
        PE::Break(name_opt, pe_opt) => {
            let ev = match pe_opt {
                None => Box::new(sp(loc, EE::Unit { trailing: false })),
                Some(pe) => exp(context, *pe),
            };
            EE::Break(name_opt, ev)
        }
        PE::Continue(name) => EE::Continue(name),
        PE::Dereference(pe) => EE::Dereference(exp(context, *pe)),
        PE::UnaryExp(op, pe) => EE::UnaryExp(op, exp(context, *pe)),
        PE::BinopExp(_pl, op, _pr) if op.value.is_spec_only() => {
            context.spec_deprecated(loc, /* is_error */ true);
            EE::UnresolvedError
        }
        e_ @ PE::BinopExp(..) => {
            process_binops!(
                (P::BinOp, Loc),
                Box<E::Exp>,
                Box::new(sp(loc, e_)),
                e,
                *e,
                sp!(loc, PE::BinopExp(lhs, op, rhs)) => { (lhs, (op, loc), rhs) },
                { exp(context, *e) },
                value_stack,
                (bop, loc) => {
                    let el = value_stack.pop().expect("ICE binop expansion issue");
                    let er = value_stack.pop().expect("ICE binop expansion issue");
                    Box::new(sp(loc, EE::BinopExp(el, bop, er)))
                }
            )
            .value
        }
        PE::Move(loc, pdotted) => move_or_copy_path(context, PathCase::Move(loc), *pdotted),
        PE::Copy(loc, pdotted) => move_or_copy_path(context, PathCase::Copy(loc), *pdotted),
        PE::Borrow(mut_, pdotted) => match exp_dotted(context, *pdotted) {
            Some(edotted) => EE::ExpDotted(E::DottedUsage::Borrow(mut_), Box::new(edotted)),
            None => {
                assert!(context.env().has_errors());
                EE::UnresolvedError
            }
        },
        pdotted_ @ PE::Dot(_, _) => match exp_dotted(context, sp(loc, pdotted_)) {
            Some(edotted) => EE::ExpDotted(E::DottedUsage::Use, Box::new(edotted)),
            None => {
                assert!(context.env().has_errors());
                EE::UnresolvedError
            }
        },
        PE::DotCall(pdotted, n, ptys_opt, sp!(rloc, prs)) => match exp_dotted(context, *pdotted) {
            Some(edotted) => {
                let pkg = context.current_package;
                context.env().check_feature(FeatureGate::DotCall, pkg, loc);
                let tys_opt = optional_types(context, ptys_opt);
                let ers = sp(rloc, exps(context, prs));
                EE::MethodCall(Box::new(edotted), n, tys_opt, ers)
            }
            None => {
                assert!(context.env().has_errors());
                EE::UnresolvedError
            }
        },
        PE::Cast(e, ty) => EE::Cast(exp(context, *e), type_(context, ty)),
        PE::Index(..) => {
            // TODO index syntax will be added
            context.spec_deprecated(loc, /* is_error */ true);
            EE::UnresolvedError
        }
        PE::Annotate(e, ty) => EE::Annotate(exp(context, *e), type_(context, ty)),
        PE::Spec(_) => {
            context.spec_deprecated(loc, /* is_error */ false);
            EE::Unit { trailing: false }
        }
        PE::UnresolvedError => EE::UnresolvedError,
    };
    sp(loc, e_)
}

// If the expression is a named block, hand back the name and a normal block. Otherwise, just
// process the expression. This is used to lift names for loop and while to the appropriate form.
fn maybe_named_loop(
    context: &mut Context,
    loc: Loc,
    body: P::Exp,
) -> (Option<BlockLabel>, Box<E::Exp>) {
    if let P::Exp_::NamedBlock(name, seq) = body.value {
        (
            Some(name),
            Box::new(sp(loc, E::Exp_::Block(sequence(context, loc, seq)))),
        )
    } else {
        (None, exp(context, body))
    }
}

#[derive(Copy, Clone)]
enum PathCase {
    Move(Loc),
    Copy(Loc),
}

impl PathCase {
    fn loc(self) -> Loc {
        match self {
            PathCase::Move(loc) | PathCase::Copy(loc) => loc,
        }
    }
    fn case(self) -> &'static str {
        match self {
            PathCase::Move(_) => "move",
            PathCase::Copy(_) => "copy",
        }
    }
}

fn move_or_copy_path(context: &mut Context, case: PathCase, pe: P::Exp) -> E::Exp_ {
    match move_or_copy_path_(context, case, pe) {
        Some(e) => e,
        None => {
            assert!(context.env().has_errors());
            E::Exp_::UnresolvedError
        }
    }
}

fn move_or_copy_path_(context: &mut Context, case: PathCase, pe: P::Exp) -> Option<E::Exp_> {
    let e = exp_dotted(context, pe)?;
    let cloc = case.loc();
    match &e.value {
        E::ExpDotted_::Exp(inner) => {
            if !matches!(&inner.value, E::Exp_::Name(_, _)) {
                let cmsg = format!("Invalid '{}' of expression", case.case());
                let emsg = "Expected a name or path access, e.g. 'x' or 'e.f'";
                context.env().add_diag(diag!(
                    Syntax::InvalidMoveOrCopy,
                    (cloc, cmsg),
                    (inner.loc, emsg)
                ));
                return None;
            }
        }
        E::ExpDotted_::Dot(_, _) => {
            let current_package = context.current_package;
            context
                .env()
                .check_feature(FeatureGate::Move2024Paths, current_package, cloc);
        }
    }
    Some(match case {
        PathCase::Move(loc) => E::Exp_::ExpDotted(E::DottedUsage::Move(loc), Box::new(e)),
        PathCase::Copy(loc) => E::Exp_::ExpDotted(E::DottedUsage::Copy(loc), Box::new(e)),
    })
}

fn exp_dotted(context: &mut Context, sp!(loc, pdotted_): P::Exp) -> Option<E::ExpDotted> {
    use E::ExpDotted_ as EE;
    use P::Exp_ as PE;
    let edotted_ = match pdotted_ {
        PE::Dot(plhs, field) => {
            let lhs = exp_dotted(context, *plhs)?;
            EE::Dot(Box::new(lhs), field)
        }
        pe_ => EE::Exp(Box::new(exp_(context, sp(loc, pe_)))),
    };
    Some(sp(loc, edotted_))
}

//**************************************************************************************************
// Match and Patterns
//**************************************************************************************************

fn match_arm(context: &mut Context, sp!(loc, arm_): P::MatchArm) -> E::MatchArm {
    let P::MatchArm_ {
        pattern,
        guard,
        rhs,
    } = arm_;
    let pattern = match_pattern(context, pattern);
    let binders = pattern_binders(context, &pattern);
    let guard = guard.map(|guard| exp(context, *guard));
    let rhs = exp(context, *rhs);
    let arm = E::MatchArm_ {
        pattern,
        binders,
        guard,
        rhs,
    };
    sp(loc, arm)
}

fn match_pattern(context: &mut Context, sp!(loc, pat_): P::MatchPattern) -> E::MatchPattern {
    use E::{MatchPattern_ as EP, ModuleAccess_ as EM};
    use P::MatchPattern_ as PP;

    fn head_ctor_okay(
        context: &mut Context,
        name: E::ModuleAccess,
        identifier_okay: bool,
    ) -> Option<E::ModuleAccess> {
        match &name.value {
            EM::Variant(_, _) => Some(name),
            EM::Name(_) if identifier_okay => Some(name),
            EM::Name(_) => {
                context.env().add_diag(diag!(
                    Syntax::UnexpectedToken,
                    (
                        name.loc,
                        "Unexpected name access.\
                        Expected an '<enum>::<variant>' form."
                    )
                ));
                None
            }
            EM::ModuleAccess(_mident, name) => {
                context.env().add_diag(diag!(
                    Syntax::UnexpectedToken,
                    (
                        name.loc,
                        "Unexpected module member access.\
                        Expected an identifier or enum variant."
                    )
                ));
                None
            }
        }
    }

    macro_rules! error_pattern {
        () => {{
            assert!(context.env().has_errors());
            sp(loc, EP::Wildcard)
        }};
    }

    match pat_ {
        PP::PositionalConstructor(name_chain, pts_opt, pats) => {
            let head_ctor_name = context
                .name_access_chain_to_module_access(Access::Variant, name_chain)
                .and_then(|name| head_ctor_okay(context, name, false));
            let tys = optional_types(context, pts_opt);
            match head_ctor_name {
                Some(head_ctor_name @ sp!(_, EM::Variant(_, _))) => {
                    let ploc = pats.loc;
                    let pats = pats
                        .value
                        .into_iter()
                        .map(|pat| match_pattern(context, pat))
                        .collect();
                    sp(
                        loc,
                        EP::PositionalConstructor(head_ctor_name, tys, sp(ploc, pats)),
                    )
                }
                _ => error_pattern!(),
            }
        }
        PP::FieldConstructor(name_chain, pts_opt, fields) => {
            let head_ctor_name = context
                .name_access_chain_to_module_access(Access::Variant, name_chain)
                .and_then(|name| head_ctor_okay(context, name, false));
            let tys = optional_types(context, pts_opt);
            match head_ctor_name {
                Some(head_ctor_name @ sp!(_, EM::Variant(_, _))) => {
                    let fields = fields
                        .value
                        .into_iter()
                        .map(|(field, pat)| (field, match_pattern(context, pat)))
                        .collect();
                    let fields = named_fields(context, loc, "pattern", "sub-pattern", fields);
                    sp(loc, EP::FieldConstructor(head_ctor_name, tys, fields))
                }
                _ => error_pattern!(),
            }
        }
        PP::Name(name_chain, pts_opt) => {
            let head_ctor_name = context
                .name_access_chain_to_module_access(Access::Variant, name_chain)
                .and_then(|name| head_ctor_okay(context, name, true));
            let tys = optional_types(context, pts_opt);
            match head_ctor_name {
                Some(sp!(loc, EM::Name(name))) => sp(loc, EP::Binder(Var(name))),
                Some(head_ctor_name @ sp!(_, EM::Variant(_, _))) => {
                    sp(loc, EP::HeadConstructor(head_ctor_name, tys))
                }
                _ => error_pattern!(),
            }
        }
        PP::Literal(v) => {
            if let Some(v) = value(&mut context.defn_context, v) {
                sp(loc, EP::Literal(v))
            } else {
                assert!(context.env().has_errors());
                sp(loc, EP::Wildcard)
            }
        }
        PP::Wildcard => sp(loc, EP::Wildcard),
        PP::Or(lhs, rhs) => sp(
            loc,
            EP::Or(
                Box::new(match_pattern(context, *lhs)),
                Box::new(match_pattern(context, *rhs)),
            ),
        ),
        PP::At(x, inner) => sp(loc, EP::At(x, Box::new(match_pattern(context, *inner)))),
    }
}

fn pattern_binders(context: &mut Context, pattern: &E::MatchPattern) -> Vec<Var> {
    use E::MatchPattern_ as EP;

    fn report_duplicate(context: &mut Context, var: Var, locs: &Vec<Loc>) {
        assert!(locs.len() > 1, "ICE pattern duplicate detection error");
        let first_loc = locs.first().unwrap();
        let mut diag = diag!(
            NameResolution::InvalidPattern,
            (*first_loc, format!("binder '{}' is defined here", var))
        );
        for loc in locs.iter().skip(1) {
            diag.add_secondary_label((*loc, "and repeated here"));
        }
        diag.add_note("A pattern variable must be unique unless it appears on different sides of an or-pattern.");
        context.env().add_diag(diag);
    }

    enum OrPosn {
        Left,
        Right,
    }

    fn report_mismatched_or(context: &mut Context, posn: OrPosn, var: &Var, other_loc: Loc) {
        let (primary_side, secondary_side) = match posn {
            OrPosn::Left => ("left", "right"),
            OrPosn::Right => ("right", "left"),
        };
        let primary_msg = format!("{} or-pattern binds variable {}", primary_side, var);
        let secondary_msg = format!("{} or-pattern does not", secondary_side);
        let mut diag = diag!(NameResolution::InvalidPattern, (var.loc(), primary_msg));
        diag.add_secondary_label((other_loc, secondary_msg));
        diag.add_note("Both sides of an or-pattern must bind the same variables.");
        context.env().add_diag(diag);
    }

    type Bindings = BTreeMap<Var, Vec<Loc>>;

    fn report_duplicates_and_combine(
        context: &mut Context,
        all_bindings: Vec<Bindings>,
    ) -> Bindings {
        match all_bindings.len() {
            0 => BTreeMap::new(),
            1 => all_bindings[0].clone(),
            _ => {
                let mut out_bindings = all_bindings[0].clone();
                let mut duplicates = BTreeSet::new();
                for bindings in all_bindings.into_iter().skip(1) {
                    for (key, mut locs) in bindings {
                        if out_bindings.contains_key(&key) {
                            duplicates.insert(key);
                        }
                        out_bindings.entry(key).or_default().append(&mut locs);
                    }
                }
                for key in duplicates {
                    report_duplicate(context, key, out_bindings.get(&key).unwrap());
                }
                out_bindings
            }
        }
    }

    fn check_duplicates(context: &mut Context, sp!(ploc, pattern): &E::MatchPattern) -> Bindings {
        match pattern {
            EP::Binder(var) | EP::At(var, _) => {
                let mut bindings: Bindings = BTreeMap::new();
                bindings.entry(*var).or_default().push(*ploc);
                if let EP::At(_, inner) = pattern {
                    let new_bindings = check_duplicates(context, inner);
                    bindings = report_duplicates_and_combine(context, vec![bindings, new_bindings]);
                }
                bindings
            }
            EP::PositionalConstructor(_, _, sp!(_, patterns)) => {
                let bindings = patterns
                    .iter()
                    .map(|pat| check_duplicates(context, pat))
                    .collect();
                report_duplicates_and_combine(context, bindings)
            }
            EP::FieldConstructor(_, _, fields) => {
                let mut bindings = vec![];
                for (_, _, (_, pat)) in fields {
                    bindings.push(check_duplicates(context, pat));
                }
                report_duplicates_and_combine(context, bindings)
            }
            EP::Or(left, right) => {
                let mut left_bindings = check_duplicates(context, left);
                let mut right_bindings = check_duplicates(context, right);

                for key in left_bindings.keys() {
                    if !right_bindings.contains_key(key) {
                        report_mismatched_or(context, OrPosn::Left, key, right.loc);
                    }
                }

                let right_keys = right_bindings.keys().copied().collect::<Vec<_>>();
                for key in right_keys {
                    let lhs_entry = left_bindings.get_mut(&key);
                    let rhs_entry = right_bindings.remove(&key);
                    match (lhs_entry, rhs_entry) {
                        (Some(left_locs), Some(mut right_locs)) => {
                            left_locs.append(&mut right_locs);
                        }
                        (None, Some(right_locs)) => {
                            report_mismatched_or(context, OrPosn::Right, &key, left.loc);
                            left_bindings.insert(key, right_locs);
                        }
                        (_, None) => panic!("ICE pattern key missing"),
                    }
                }
                left_bindings
            }
            EP::HeadConstructor(_, _) | EP::Wildcard | EP::Literal(_) => BTreeMap::new(),
        }
    }

    let bindings = check_duplicates(context, pattern);
    bindings.keys().cloned().collect::<Vec<_>>()
}

//**************************************************************************************************
// Values
//**************************************************************************************************

fn value(context: &mut DefnContext, sp!(loc, pvalue_): P::Value) -> Option<E::Value> {
    use E::Value_ as EV;
    use P::Value_ as PV;
    let value_ = match pvalue_ {
        PV::Address(addr) => {
            let addr = top_level_address(context, /* suggest_declaration */ true, addr);
            EV::Address(addr)
        }
        PV::Num(s) if s.ends_with("u8") => match parse_u8(&s[..s.len() - 2]) {
            Ok((u, _format)) => EV::U8(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u8'"));
                return None;
            }
        },
        PV::Num(s) if s.ends_with("u16") => match parse_u16(&s[..s.len() - 3]) {
            Ok((u, _format)) => EV::U16(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u16'"));
                return None;
            }
        },
        PV::Num(s) if s.ends_with("u32") => match parse_u32(&s[..s.len() - 3]) {
            Ok((u, _format)) => EV::U32(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u32'"));
                return None;
            }
        },
        PV::Num(s) if s.ends_with("u64") => match parse_u64(&s[..s.len() - 3]) {
            Ok((u, _format)) => EV::U64(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u64'"));
                return None;
            }
        },
        PV::Num(s) if s.ends_with("u128") => match parse_u128(&s[..s.len() - 4]) {
            Ok((u, _format)) => EV::U128(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u128'"));
                return None;
            }
        },
        PV::Num(s) if s.ends_with("u256") => match parse_u256(&s[..s.len() - 4]) {
            Ok((u, _format)) => EV::U256(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(loc, "'u256'"));
                return None;
            }
        },

        PV::Num(s) => match parse_u256(&s) {
            Ok((u, _format)) => EV::InferredNum(u),
            Err(_) => {
                context.env.add_diag(num_too_big_error(
                    loc,
                    "the largest possible integer type, 'u256'",
                ));
                return None;
            }
        },
        PV::Bool(b) => EV::Bool(b),
        PV::HexString(s) => match hex_string::decode(loc, &s) {
            Ok(v) => EV::Bytearray(v),
            Err(e) => {
                context.env.add_diag(*e);
                return None;
            }
        },
        PV::ByteString(s) => match byte_string::decode(loc, &s) {
            Ok(v) => EV::Bytearray(v),
            Err(e) => {
                context.env.add_diags(e);
                return None;
            }
        },
    };
    Some(sp(loc, value_))
}

// Create an error for an integer literal that is too big to fit in its type.
// This assumes that the literal is the current token.
fn num_too_big_error(loc: Loc, type_description: &'static str) -> Diagnostic {
    diag!(
        Syntax::InvalidNumber,
        (
            loc,
            format!(
                "Invalid number literal. The given literal is too large to fit into {}",
                type_description
            )
        ),
    )
}

//**************************************************************************************************
// Fields
//**************************************************************************************************

fn named_fields<T>(
    context: &mut Context,
    loc: Loc,
    case: &str,
    verb: &str,
    xs: Vec<(Field, T)>,
) -> Fields<T> {
    let mut fmap = UniqueMap::new();
    for (idx, (field, x)) in xs.into_iter().enumerate() {
        if let Err((field, old_loc)) = fmap.add(field, (idx, x)) {
            context.env().add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, format!("Invalid {}", case)),
                (
                    field.loc(),
                    format!("Duplicate {} given for field '{}'", verb, field),
                ),
                (old_loc, "Field previously defined here".into()),
            ))
        }
    }
    fmap
}

//**************************************************************************************************
// LValues
//**************************************************************************************************

fn bind_list(context: &mut Context, sp!(loc, pbs_): P::BindList) -> Option<E::LValueList> {
    let bs_: Option<Vec<E::LValue>> = pbs_.into_iter().map(|pb| bind(context, pb)).collect();
    Some(sp(loc, bs_?))
}

fn bind(context: &mut Context, sp!(loc, pb_): P::Bind) -> Option<E::LValue> {
    use E::LValue_ as EL;
    use P::Bind_ as PB;
    let b_ = match pb_ {
        PB::Var(pmut, v) => {
            let emut = mutability(context, v.loc(), pmut);
            check_valid_local_name(context, &v);
            EL::Var(emut, sp(loc, E::ModuleAccess_::Name(v.0)), None)
        }
        PB::Unpack(ptn, ptys_opt, pfields) => {
            let tn = context.name_access_chain_to_module_access(Access::ApplyNamed, *ptn)?;
            let tys_opt = optional_types(context, ptys_opt);
            let fields = match pfields {
                FieldBindings::Named(named_bindings) => {
                    let vfields: Option<Vec<(Field, E::LValue)>> = named_bindings
                        .into_iter()
                        .map(|(f, pb)| Some((f, bind(context, pb)?)))
                        .collect();
                    let fields =
                        named_fields(context, loc, "deconstruction binding", "binding", vfields?);
                    E::FieldBindings::Named(fields)
                }
                FieldBindings::Positional(positional_bindings) => {
                    let fields: Option<Vec<E::LValue>> = positional_bindings
                        .into_iter()
                        .map(|b| bind(context, b))
                        .collect();
                    E::FieldBindings::Positional(fields?)
                }
            };
            EL::Unpack(tn, tys_opt, fields)
        }
    };
    Some(sp(loc, b_))
}

enum LValue {
    Assigns(E::LValueList),
    FieldMutate(Box<E::ExpDotted>),
    Mutate(Box<E::Exp>),
}

fn lvalues(context: &mut Context, sp!(loc, e_): P::Exp) -> Option<LValue> {
    use LValue as L;
    use P::Exp_ as PE;
    let al: LValue = match e_ {
        PE::Unit => L::Assigns(sp(loc, vec![])),
        PE::ExpList(pes) => {
            let al_opt: Option<E::LValueList_> =
                pes.into_iter().map(|pe| assign(context, pe)).collect();
            L::Assigns(sp(loc, al_opt?))
        }
        PE::Dereference(pr) => {
            let er = exp(context, *pr);
            L::Mutate(er)
        }
        pdotted_ @ PE::Dot(_, _) => {
            let dotted = exp_dotted(context, sp(loc, pdotted_))?;
            L::FieldMutate(Box::new(dotted))
        }
        _ => L::Assigns(sp(loc, vec![assign(context, sp(loc, e_))?])),
    };
    Some(al)
}

fn assign(context: &mut Context, sp!(loc, e_): P::Exp) -> Option<E::LValue> {
    use E::LValue_ as EL;
    use E::ModuleAccess_ as M;
    use P::Exp_ as PE;
    match e_ {
        PE::Name(name, ptys_opt) => {
            let resolved_name =
                context.name_access_chain_to_module_access(Access::Term, name.clone());
            match resolved_name {
                Some(sp!(_, M::Name(_))) if ptys_opt.is_some() => {
                    let msg = "Unexpected assignment of instantiated type without fields";
                    let mut diag = diag!(Syntax::InvalidLValue, (loc, msg));
                    diag.add_note(format!(
                        "If you are trying to unpack a struct, try adding fields, e.g.'{} {{}}'",
                        name
                    ));
                    context.env().add_diag(diag);
                    None
                }
                Some(sp!(_, M::ModuleAccess(_, _))) => {
                    let msg = "Unexpected assignment of module access without fields";
                    let mut diag = diag!(Syntax::InvalidLValue, (loc, msg));
                    diag.add_note(format!(
                        "If you are trying to unpack a struct, try adding fields, e.g.'{} {{}}'",
                        name
                    ));
                    context.env().add_diag(diag);
                    None
                }
                Some(sp!(loc, M::Variant(_, _))) => {
                    let cur_pkg = context.current_package;
                    if context
                        .env()
                        .check_feature(FeatureGate::Enums, cur_pkg, loc)
                    {
                        let msg = "Unexpected assignment of variant";
                        let mut diag = diag!(Syntax::InvalidLValue, (loc, msg));
                        diag.add_note("If you are trying to unpack an enum variant, use 'match'");
                        context.env().add_diag(diag);
                        None
                    } else {
                        assert!(context.env().has_errors());
                        None
                    }
                }
                Some(sp!(_, name @ M::Name(_))) => {
                    Some(sp(loc, EL::Var(None, sp(loc, name), None)))
                }
                None => None,
            }
        }
        PE::Pack(pn, ptys_opt, pfields) => {
            let en = context.name_access_chain_to_module_access(Access::ApplyNamed, pn)?;
            let tys_opt = optional_types(context, ptys_opt);
            let efields = assign_unpack_fields(context, loc, pfields)?;
            Some(sp(
                loc,
                EL::Unpack(en, tys_opt, E::FieldBindings::Named(efields)),
            ))
        }
        PE::Call(pn, false, ptys_opt, sp!(_, exprs)) => {
            let pkg = context.current_package;
            context
                .env()
                .check_feature(FeatureGate::PositionalFields, pkg, loc);
            let en = context.name_access_chain_to_module_access(Access::ApplyNamed, pn)?;
            let tys_opt = optional_types(context, ptys_opt);
            let pfields: Option<_> = exprs.into_iter().map(|e| assign(context, e)).collect();
            Some(sp(
                loc,
                EL::Unpack(en, tys_opt, E::FieldBindings::Positional(pfields?)),
            ))
        }
        _ => {
            context.env().add_diag(diag!(
                Syntax::InvalidLValue,
                (
                    loc,
                    "Invalid assignment syntax. Expected: a local, a field write, or a \
                     deconstructing assignment"
                )
            ));
            None
        }
    }
}

fn assign_unpack_fields(
    context: &mut Context,
    loc: Loc,
    pfields: Vec<(Field, P::Exp)>,
) -> Option<Fields<E::LValue>> {
    let afields = pfields
        .into_iter()
        .map(|(f, e)| Some((f, assign(context, e)?)))
        .collect::<Option<_>>()?;
    Some(named_fields(
        context,
        loc,
        "deconstructing assignment",
        "assignment binding",
        afields,
    ))
}

fn mutability(context: &mut Context, loc: Loc, pmut: Mutability) -> Mutability {
    let supports_let_mut = {
        let pkg = context.current_package;
        context.env().supports_feature(pkg, FeatureGate::LetMut)
    };
    match pmut {
        Some(loc) => {
            assert!(supports_let_mut, "ICE mut should not parse without let mut");
            Some(loc)
        }
        None if supports_let_mut => None,
        // without let mut enabled, all locals are mutable and do not need the annotation
        None => Some(loc),
    }
}

//**************************************************************************************************
// Valid names
//**************************************************************************************************

fn check_valid_address_name(
    context: &mut DefnContext,
    sp!(_, ln_): &P::LeadingNameAccess,
) -> Result<(), ()> {
    use P::LeadingNameAccess_ as LN;
    match ln_ {
        LN::AnonymousAddress(_) => Ok(()),
        LN::GlobalAddress(n) => check_restricted_name_all_cases(context, NameCase::Address, n),
        LN::Name(n) => check_restricted_name_all_cases(context, NameCase::Address, n),
    }
}

fn check_valid_local_name(context: &mut Context, v: &Var) {
    fn is_valid(s: Symbol) -> bool {
        s.starts_with('_') || s.starts_with(|c: char| c.is_ascii_lowercase())
    }
    if !is_valid(v.value()) {
        let msg = format!(
            "Invalid local variable name '{}'. Local variable names must start with 'a'..'z' (or \
             '_')",
            v,
        );
        context
            .env()
            .add_diag(diag!(Declarations::InvalidName, (v.loc(), msg)));
    }
    let _ = check_restricted_name_all_cases(&mut context.defn_context, NameCase::Variable, &v.0);
}

#[derive(Copy, Clone, Debug)]
pub enum ModuleMemberKind {
    Constant,
    Function,
    Struct,
    Enum,
    Schema,
}

impl ModuleMemberKind {
    pub fn case(self) -> NameCase {
        match self {
            ModuleMemberKind::Constant => NameCase::Constant,
            ModuleMemberKind::Function => NameCase::Function,
            ModuleMemberKind::Struct => NameCase::Struct,
            ModuleMemberKind::Enum => NameCase::Enum,
            ModuleMemberKind::Schema => NameCase::Schema,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum NameCase {
    Constant,
    Function,
    Struct,
    Enum,
    Schema,
    Module,
    ModuleMemberAlias(ModuleMemberKind),
    ModuleAlias,
    Variable,
    Address,
}

impl NameCase {
    pub const fn name(&self) -> &'static str {
        match self {
            NameCase::Constant => "constant",
            NameCase::Function => "function",
            NameCase::Struct => "struct",
            NameCase::Enum => "enum",
            NameCase::Schema => "schema",
            NameCase::Module => "module",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Function) => "function alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Constant) => "constant alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Struct) => "struct alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Enum) => "enum alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Schema) => "schema alias",
            NameCase::ModuleAlias => "module alias",
            NameCase::Variable => "variable",
            NameCase::Address => "address",
        }
    }
}

fn check_valid_module_member_name(
    context: &mut Context,
    member: ModuleMemberKind,
    name: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(context, member, &name, member.case()) {
        Err(()) => None,
        Ok(()) => Some(name),
    }
}

fn check_valid_module_member_alias(
    context: &mut Context,
    member: ModuleMemberKind,
    alias: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(
        context,
        member,
        &alias,
        NameCase::ModuleMemberAlias(member),
    ) {
        Err(()) => None,
        Ok(()) => Some(alias),
    }
}

fn check_valid_module_member_name_impl(
    context: &mut Context,
    member: ModuleMemberKind,
    n: &Name,
    case: NameCase,
) -> Result<(), ()> {
    use ModuleMemberKind as M;
    fn upper_first_letter(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
    match member {
        M::Function => {
            if n.value.starts_with(|c| c == '_') {
                let msg = format!(
                    "Invalid {} name '{}'. {} names cannot start with '_'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                context
                    .env()
                    .add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        M::Constant | M::Struct | M::Enum | M::Schema => {
            if !is_valid_struct_constant_or_schema_name(&n.value) {
                let msg = format!(
                    "Invalid {} name '{}'. {} names must start with 'A'..'Z'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                context
                    .env()
                    .add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
    }

    // TODO move these names to a more central place?
    check_restricted_names(
        context,
        case,
        n,
        crate::naming::ast::BuiltinFunction_::all_names(),
    )?;
    check_restricted_names(
        context,
        case,
        n,
        crate::naming::ast::BuiltinTypeName_::all_names(),
    )?;

    // Restricting Self for now in the case where we ever have impls
    // Otherwise, we could allow it
    check_restricted_name_all_cases(&mut context.defn_context, case, n)?;

    Ok(())
}

pub fn is_valid_struct_constant_or_schema_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase())
}

// Checks for a restricted name in any decl case
// Self and vector are not allowed
fn check_restricted_name_all_cases(
    context: &mut DefnContext,
    case: NameCase,
    n: &Name,
) -> Result<(), ()> {
    let n_str = n.value.as_str();
    let can_be_vector = matches!(case, NameCase::Module | NameCase::ModuleAlias);
    if n_str == ModuleName::SELF_NAME
        || (!can_be_vector && n_str == crate::naming::ast::BuiltinTypeName_::VECTOR)
    {
        context
            .env
            .add_diag(restricted_name_error(case, n.loc, n_str));
        Err(())
    } else {
        Ok(())
    }
}

fn check_restricted_names(
    context: &mut Context,
    case: NameCase,
    sp!(loc, n_): &Name,
    all_names: &BTreeSet<Symbol>,
) -> Result<(), ()> {
    if all_names.contains(n_) {
        context
            .env()
            .add_diag(restricted_name_error(case, *loc, n_));
        Err(())
    } else {
        Ok(())
    }
}

fn restricted_name_error(case: NameCase, loc: Loc, restricted: &str) -> Diagnostic {
    let a_or_an = match case.name().chars().next().unwrap() {
        // TODO this is not exhaustive to the indefinite article rules in English
        // but 'case' is never user generated, so it should be okay for a while/forever...
        'a' | 'e' | 'i' | 'o' | 'u' => "an",
        _ => "a",
    };
    let msg = format!(
        "Invalid {case} name '{restricted}'. '{restricted}' is restricted and cannot be used to \
         name {a_or_an} {case}",
        a_or_an = a_or_an,
        case = case.name(),
        restricted = restricted,
    );
    diag!(NameResolution::ReservedName, (loc, msg))
}
