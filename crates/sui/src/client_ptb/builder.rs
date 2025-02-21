// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    client_commands::{compile_package, upgrade_package},
    client_ptb::{
        ast::{Argument as PTBArg, ASSIGN, GAS_BUDGET},
        error::{PTBError, PTBResult, Span, Spanned},
    },
    err, error, sp,
};
use anyhow::Result;
use async_recursion::async_recursion;
use async_trait::async_trait;
use miette::Severity;
use move_binary_format::{
    binary_config::BinaryConfig, file_format::SignatureToken, CompiledModule,
};
use move_core_types::parsing::{
    address::{NumericalAddress, ParsedAddress},
    parser::NumberFormat,
};
use move_core_types::{
    account_address::AccountAddress, annotated_value::MoveTypeLayout, ident_str,
};
use move_package::BuildConfig;
use std::{collections::BTreeMap, path::Path};
use sui_json::{is_receiving_argument, primitive_type};
use sui_json_rpc_types::{SuiObjectData, SuiObjectDataOptions, SuiRawData};
use sui_move::manage_package::resolve_lock_file_path;
use sui_sdk::apis::ReadApi;
use sui_types::{
    base_types::{is_primitive_type_tag, ObjectID, TxContext, TxContextKind},
    move_package::MovePackage,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    resolve_address,
    transaction::{self as Tx, ObjectArg},
    Identifier, TypeTag, SUI_FRAMEWORK_PACKAGE_ID,
};

use super::ast::{ModuleAccess as PTBModuleAccess, ParsedPTBCommand, Program};

// ===========================================================================
// Object Resolution
// ===========================================================================
// We need to resolve the same argument in different ways depending on the context in which that
// argument is used. For example, if we are using an object ID as an argument to a move call that
// expects an object argument in that position, the object ID should be resolved to an object,
// whereas if we are using the same object ID as an argument to a pure value, it should be resolved
// to a pure value (e.g., the object ID itself). The different ways of resolving this is the
// purpose of the `Resolver` trait -- different contexts will implement this trait in different
// ways.

/// A resolver is used to resolve arguments to a PTB. Depending on the context, we may resolve
/// object IDs in different ways -- e.g., in a pure context they should be resolved to a pure
/// value, whereas in an object context they should be resolved to the appropriate object argument.
#[async_trait]
trait Resolver<'a>: Send {
    /// Resolve a pure value. This should almost always resolve to a pure value.
    async fn pure(
        &mut self,
        builder: &mut PTBBuilder<'a>,
        loc: Span,
        argument: PTBArg,
    ) -> PTBResult<Tx::Argument> {
        let value = argument.to_pure_move_value(loc)?;
        builder.ptb.pure(value).map_err(|e| err!(loc, "{e}"))
    }

    async fn resolve_object_id(
        &mut self,
        builder: &mut PTBBuilder<'a>,
        loc: Span,
        obj_id: ObjectID,
    ) -> PTBResult<Tx::Argument>;

    fn re_resolve(&self) -> bool {
        false
    }
}

/// A resolver that resolves object IDs to object arguments.
/// * If `is_receiving` is true, then the object argument will be resolved to a receiving object
///   argument.
/// * If `is_mut` is true, then the object argument will be resolved to a mutable object argument.
struct ToObject {
    is_receiving: bool,
    is_mut: bool,
}

impl Default for ToObject {
    fn default() -> Self {
        Self {
            is_receiving: false,
            is_mut: true,
        }
    }
}

impl ToObject {
    fn new(is_receiving: bool, is_mut: bool) -> Self {
        Self {
            is_receiving,
            is_mut,
        }
    }
}

#[async_trait]
impl<'a> Resolver<'a> for ToObject {
    async fn resolve_object_id(
        &mut self,
        builder: &mut PTBBuilder<'a>,
        loc: Span,
        obj_id: ObjectID,
    ) -> PTBResult<Tx::Argument> {
        // Get the object from the reader to get metadata about the object.
        let obj = builder.get_object(obj_id, loc).await?;
        let owner = obj
            .owner
            .clone()
            .ok_or_else(|| err!(loc, "Unable to get owner info for object {obj_id}"))?;
        let object_ref = obj.object_ref();
        // Depending on the ownership of the object, we resolve it to different types of object
        // arguments for the transaction.
        let obj_arg = match owner {
            Owner::AddressOwner(_) if self.is_receiving => ObjectArg::Receiving(object_ref),
            Owner::Immutable | Owner::AddressOwner(_) => ObjectArg::ImmOrOwnedObject(object_ref),
            Owner::Shared {
                initial_shared_version,
            }
            | Owner::ConsensusV2 {
                start_version: initial_shared_version,
                ..
            } => ObjectArg::SharedObject {
                id: object_ref.0,
                initial_shared_version,
                mutable: self.is_mut,
            },
            Owner::ObjectOwner(_) => {
                error!(loc => help: {
                    "{obj_id} is an object-owned object, you can only use immutable, shared, or owned objects here."
                }, "Cannot use an object-owned object as an argument")
            }
        };
        // Insert the correct object arg that we built above into the transaction.
        builder.ptb.obj(obj_arg).map_err(|e| err!(loc, "{e}"))
    }

    // We always re-resolve object IDs to object arguments if we need it mutably -- we could have
    // added it earlier as an immutable argument.
    fn re_resolve(&self) -> bool {
        self.is_mut
    }
}

/// A resolver that resolves object IDs that it encounters to pure PTB values.
struct ToPure {
    type_: TypeTag,
}

impl ToPure {
    pub fn new(type_: TypeTag) -> Self {
        Self { type_ }
    }

    pub fn new_from_layout(layout: MoveTypeLayout) -> Self {
        Self {
            type_: TypeTag::from(&layout),
        }
    }
}

#[async_trait]
impl<'a> Resolver<'a> for ToPure {
    async fn pure(
        &mut self,
        builder: &mut PTBBuilder<'a>,
        loc: Span,
        argument: PTBArg,
    ) -> PTBResult<Tx::Argument> {
        let value = argument.checked_to_pure_move_value(loc, &self.type_)?;
        builder.ptb.pure(value).map_err(|e| err!(loc, "{e}"))
    }

    async fn resolve_object_id(
        &mut self,
        builder: &mut PTBBuilder<'a>,
        loc: Span,
        obj_id: ObjectID,
    ) -> PTBResult<Tx::Argument> {
        builder.ptb.pure(obj_id).map_err(|e| err!(loc, "{e}"))
    }
}

// ===========================================================================
// PTB Builder and PTB Creation
// ===========================================================================

/// The PTBBuilder struct is the main workhorse that transforms a sequence of `ParsedPTBCommand`s
/// into an actual PTB that can be run. The main things to keep in mind are that this contains:
/// - A way to handle identifiers -- note that we "lazily" resolve identifiers to arguments, so
///   that the first usage of the identifier determines what it is resolved to. If an identifier is
///   used in multiple positions at different resolutions (e.g., in one place as an object argument,
///   and in another as a pure value), this will result in an error. This error can be avoided by
///   creating another identifier for the second usage.
/// - A way to resolve arguments -- this is done by calling `resolve` on a `PTBArg` and passing in
///   appropriate context. The context is used to determine how to resolve the argument -- e.g., if
///   an object ID should be resolved to a pure value or an object argument.
/// - A way to bind the result of a command to an identifier.
pub struct PTBBuilder<'a> {
    /// A map from identifiers to addresses. This is used to support address resolution, and also
    /// supports external address sources (e.g., keystore).
    addresses: BTreeMap<String, AccountAddress>,
    /// A map from identifiers to the file scopes in which they were declared. This is used
    /// for reporting shadowing warnings.
    identifiers: BTreeMap<String, Vec<Span>>,
    /// The arguments that we need to resolve. This is a map from identifiers to the argument
    /// values -- they haven't been resolved to a transaction argument yet.
    arguments_to_resolve: BTreeMap<String, ArgWithHistory>,
    /// The arguments that we have resolved. This is a map from identifiers to the actual
    /// transaction arguments.
    resolved_arguments: BTreeMap<String, Tx::Argument>,
    /// Read API for reading objects from chain. Needed for object resolution.
    reader: &'a ReadApi,
    /// The last command that we have added. This is used to support assignment commands.
    last_command: Option<Tx::Argument>,
    /// The actual PTB that we are building up.
    ptb: ProgrammableTransactionBuilder,
    /// The list of errors that we have built up while processing commands. We do not report errors
    /// eagerly but instead wait until we have processed all commands to report any errors.
    errors: Vec<PTBError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedAccess {
    ResultAccess(u16),
    DottedString(String),
}

/// Hold a PTB argument, always remembering its most recent state even if it's already been
/// resolved.
#[derive(Debug)]
enum ArgWithHistory {
    Resolved(Spanned<PTBArg>),
    Unresolved(Spanned<PTBArg>),
}

impl ArgWithHistory {
    fn get_unresolved(&self) -> Option<&Spanned<PTBArg>> {
        match self {
            ArgWithHistory::Resolved(_) => None,
            ArgWithHistory::Unresolved(x) => Some(x),
        }
    }

    fn get(&self) -> &Spanned<PTBArg> {
        match self {
            ArgWithHistory::Resolved(x) => x,
            ArgWithHistory::Unresolved(x) => x,
        }
    }

    fn resolve(&mut self) {
        *self = match self {
            ArgWithHistory::Resolved(x) => ArgWithHistory::Resolved(x.clone()),
            ArgWithHistory::Unresolved(x) => ArgWithHistory::Resolved(x.clone()),
        }
    }

    fn is_resolved(&self) -> bool {
        matches!(self, ArgWithHistory::Resolved(_))
    }
}

impl<'a> PTBBuilder<'a> {
    pub fn new(starting_env: BTreeMap<String, AccountAddress>, reader: &'a ReadApi) -> Self {
        Self {
            addresses: starting_env,
            identifiers: BTreeMap::new(),
            arguments_to_resolve: BTreeMap::new(),
            resolved_arguments: BTreeMap::new(),
            ptb: ProgrammableTransactionBuilder::new(),
            reader,
            last_command: None,
            errors: Vec::new(),
        }
    }

    /// Finalize a PTB. If there were errors during the construction of the PTB these are returned
    /// now. Otherwise, the PTB is finalized and returned.
    /// If the warn_on_shadowing flag was set, then we will print warnings for any shadowed
    /// variables that we encountered during the building of the PTB.
    pub fn finish(
        self,
        warn_on_shadowing: bool,
    ) -> (
        Result<Tx::ProgrammableTransaction, Vec<PTBError>>,
        Vec<PTBError>,
    ) {
        let mut warnings = vec![];
        if warn_on_shadowing {
            for (ident, commands) in self.identifiers.iter() {
                if commands.len() == 1 {
                    continue;
                }

                for (i, command_loc) in commands.iter().enumerate() {
                    if i == 0 {
                        warnings.push(PTBError {
                            message: format!("Variable '{}' first declared here", ident),
                            span: *command_loc,
                            help: None,
                            severity: Severity::Warning,
                        });
                    } else {
                        warnings.push(PTBError {
                            message: format!(
                                "Variable '{}' used again here (shadowed) for the {} time.",
                                ident, to_ordinal_contraction(i + 1)
                            ),
                            span: *command_loc,
                            help: Some("You can either rename this variable, or do not \
                                       pass the `warn-shadows` flag to ignore these types of errors.".to_string()),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }

        if !self.errors.is_empty() {
            return (Err(self.errors), warnings);
        }

        let ptb = self.ptb.finish();
        (Ok(ptb), warnings)
    }

    pub async fn build(
        mut self,
        program: Program,
    ) -> (
        Result<Tx::ProgrammableTransaction, Vec<PTBError>>,
        Vec<PTBError>,
    ) {
        for command in program.commands.into_iter() {
            self.handle_command(command).await;
        }
        self.finish(program.warn_shadows_set)
    }

    /// Add a single PTB command to the PTB that we are building up.
    /// Errors are added to the `errors` field of the PTBBuilder.
    async fn handle_command(&mut self, sp!(span, command): Spanned<ParsedPTBCommand>) {
        if let Err(e) = self.handle_command_(span, command).await {
            self.errors.push(e);
        }
    }

    // ===========================================================================
    // Declaring and handling identifiers and variables
    // ===========================================================================

    /// Declare an identifier. This is used to support shadowing warnings.
    fn declare_identifier(&mut self, ident: String, ident_loc: Span) {
        let e = self.identifiers.entry(ident).or_default();
        e.push(ident_loc);
    }

    /// Declare a possible address binding. This is used to support address resolution. If the
    /// `possible_addr` is not an address, then this is a no-op.
    fn declare_possible_address_binding(&mut self, ident: String, possible_addr: &Spanned<PTBArg>) {
        match possible_addr.value {
            PTBArg::Address(addr) => {
                self.addresses.insert(ident, addr.into_inner());
            }
            PTBArg::Identifier(ref i) => {
                // We do a one-hop resolution here to see if we can resolve the identifier to an
                // externally-bound address (i.e., one coming in through the initial environment).
                // This will also handle direct aliasing of addresses throughout the ptb.
                // Note that we don't do this recursively so no need to worry about loops/cycles.
                if let Some(addr) = self.addresses.get(i) {
                    self.addresses.insert(ident, *addr);
                }
            }
            // If we encounter a dotted string e.g., "foo.0" or "sui.io" or something like that
            // this see if we can find an address for it in the environment and bind to it.
            PTBArg::VariableAccess(ref head, ref fields) => {
                let key = format!(
                    "{}.{}",
                    head.value,
                    fields
                        .iter()
                        .map(|f| f.value.clone())
                        .collect::<Vec<_>>()
                        .join(".")
                );
                if let Some(addr) = self.addresses.get(&key) {
                    self.addresses.insert(ident, *addr);
                }
            }
            _ => (),
        }
    }

    /// Resolve an object ID to a Move package.
    async fn resolve_to_package(
        &mut self,
        package_id: ObjectID,
        loc: Span,
    ) -> PTBResult<MovePackage> {
        let object = self
            .reader
            .get_object_with_options(package_id, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(|e| err!(loc, "{e}"))?
            .into_object()
            .map_err(|e| err!(loc, "{e}"))?;

        let Some(SuiRawData::Package(package)) = object.bcs else {
            error!(
                loc,
                "BCS field in object '{}' is missing or not a package.", package_id
            );
        };

        MovePackage::new(
            package.id,
            package.version,
            package.module_map,
            // This package came from on-chain and the tool runs locally, so don't worry about
            // trying to enforce the package size limit.
            u64::MAX,
            package.type_origin_table,
            package.linkage_table,
        )
        .map_err(|e| err!(loc, "{e}"))
    }

    /// Resolves the argument to the move call based on the type information of the function being
    /// called.
    async fn resolve_move_call_arg(
        &mut self,
        view: &CompiledModule,
        ty_args: &[TypeTag],
        sp!(loc, arg): Spanned<PTBArg>,
        param: &SignatureToken,
    ) -> PTBResult<Tx::Argument> {
        let layout = primitive_type(view, ty_args, param);

        // If it's a primitive value, see if we've already resolved this argument. Otherwise, we
        // need to resolve it.
        if let Some(layout) = layout {
            return self
                .resolve(loc.wrap(arg), ToPure::new_from_layout(layout))
                .await;
        }

        // Otherwise it's ambiguous what the value should be, and we need to turn to the signature
        // to determine it.
        let mut is_receiving = false;
        // A value is mutable by default.
        let mut is_mutable = true;

        // traverse the types in the signature to see if the argument is an object argument or not,
        // and also determine if it's a receiving argument or not.
        for tok in param.preorder_traversal() {
            match tok {
                SignatureToken::Datatype(..) | SignatureToken::DatatypeInstantiation(..) => {
                    is_receiving |= is_receiving_argument(view, tok);
                }
                SignatureToken::TypeParameter(idx) => {
                    if *idx as usize >= ty_args.len() {
                        error!(loc, "Not enough type parameters supplied for Move call");
                    }
                }
                SignatureToken::Reference(_) => {
                    is_mutable = false;
                }
                SignatureToken::MutableReference(_) => {
                    // Not strictly needed, but for clarity
                    is_mutable = true;
                }
                SignatureToken::Bool
                | SignatureToken::U8
                | SignatureToken::U64
                | SignatureToken::U128
                | SignatureToken::Address
                | SignatureToken::Signer
                | SignatureToken::Vector(_)
                | SignatureToken::U16
                | SignatureToken::U32
                | SignatureToken::U256 => {
                    is_mutable = false;
                }
            }
        }

        // Note: need to re-resolve an argument possibly since it may be used immutably first, and
        // then mutably.
        self.resolve(loc.wrap(arg), ToObject::new(is_receiving, is_mutable))
            .await
    }

    /// Resolve the arguments to a Move call based on the type information about the function
    /// being called.
    async fn resolve_move_call_args(
        &mut self,
        package: MovePackage,
        sp!(mloc, module_name): &Spanned<Identifier>,
        sp!(floc, function_name): &Spanned<Identifier>,
        ty_args: &[TypeTag],
        args: Vec<Spanned<PTBArg>>,
        package_name_loc: Span,
    ) -> PTBResult<Vec<Tx::Argument>> {
        let module = package
            .deserialize_module(module_name, &BinaryConfig::standard())
            .map_err(|e| {
                let help_message = if package.serialized_module_map().is_empty() {
                    Some("No modules found in this package".to_string())
                } else {
                    display_did_you_mean(find_did_you_means(
                        module_name.as_str(),
                        package
                            .serialized_module_map()
                            .iter()
                            .map(|(x, _)| x.as_str()),
                    ))
                };
                let e = err!(*mloc, "{e}");
                if let Some(help_message) = help_message {
                    e.with_help(help_message)
                } else {
                    e
                }
            })?;
        let fdef = module
            .function_defs
            .iter()
            .find(|fdef| {
                module.identifier_at(module.function_handle_at(fdef.function).name)
                    == function_name.as_ident_str()
            })
            .ok_or_else(|| {
                let e = err!(
                    *floc,
                    "Could not resolve function '{}' in module '{}'",
                    function_name,
                    module_name
                );
                if let Some(help_message) = display_did_you_mean(find_did_you_means(
                    function_name.as_str(),
                    module.function_defs.iter().map(|fdef| {
                        module
                            .identifier_at(module.function_handle_at(fdef.function).name)
                            .as_str()
                    }),
                )) {
                    e.with_help(help_message)
                } else {
                    e
                }
            })?;
        let function_signature = module.function_handle_at(fdef.function);
        let parameters: Vec<_> = module
            .signature_at(function_signature.parameters)
            .0
            .clone()
            .into_iter()
            .filter(|tok| matches!(TxContext::kind(&module, tok), TxContextKind::None))
            .collect();

        if parameters.len() != args.len() {
            let loc = if args.is_empty() {
                package_name_loc.widen(*mloc).widen(*floc)
            } else {
                args[0].span.widen_opt(args.last().map(|x| x.span))
            };
            error!(
                loc,
                "Expected {} argument{}, but got {}",
                parameters.len(),
                if parameters.len() == 1 { "" } else { "s" },
                args.len()
            );
        }

        let mut call_args = vec![];
        for (param, arg) in parameters.iter().zip(args.into_iter()) {
            let call_arg = self
                .resolve_move_call_arg(&module, ty_args, arg, param)
                .await?;
            call_args.push(call_arg);
        }
        Ok(call_args)
    }

    fn resolve_variable_access(
        &self,
        head: &Spanned<String>,
        fields: Vec<Spanned<String>>,
    ) -> Spanned<ResolvedAccess> {
        if fields.len() == 1 {
            // Get the span and value of the field zero'th field. Safe since we just checked the
            // length above. Since the length is 1, we know that the field is non-empty.
            let sp!(field_loc, field) = &fields[0];
            if let Ok(n) = field.parse::<u16>() {
                return field_loc.wrap(ResolvedAccess::ResultAccess(n));
            }
        }
        let tl_loc = head.span.widen_opt(fields.last().map(|x| x.span));
        tl_loc.wrap(ResolvedAccess::DottedString(format!(
            "{}.{}",
            head.value,
            fields
                .into_iter()
                .map(|f| f.value)
                .collect::<Vec<_>>()
                .join(".")
        )))
    }

    /// Resolve an argument based on the argument value, and the `resolver` that is passed in.
    #[async_recursion]
    async fn resolve(
        &mut self,
        sp!(arg_loc, arg): Spanned<PTBArg>,
        mut ctx: impl Resolver<'a> + 'async_recursion,
    ) -> PTBResult<Tx::Argument> {
        match arg {
            a @ (PTBArg::Bool(_)
            | PTBArg::U8(_)
            | PTBArg::U16(_)
            | PTBArg::U32(_)
            | PTBArg::U64(_)
            | PTBArg::U128(_)
            | PTBArg::U256(_)
            | PTBArg::InferredNum(_)
            | PTBArg::String(_)
            | PTBArg::Option(_)
            | PTBArg::Vector(_)) => ctx.pure(self, arg_loc, a).await,
            PTBArg::Gas => Ok(Tx::Argument::GasCoin),
            // NB: the ordering of these lines is important so that shadowing is properly
            // supported.
            // If we encounter an identifier that we have not already resolved, then we resolve the
            // value and return it.
            PTBArg::Identifier(i)
                if self
                    .arguments_to_resolve
                    .get(&i)
                    .is_some_and(|arg_hist| !arg_hist.is_resolved()) =>
            {
                let arg_hist = self.arguments_to_resolve.get(&i).unwrap();
                let arg = arg_hist.get().clone();
                let resolved = self.resolve(arg, ctx).await?;
                self.arguments_to_resolve.get_mut(&i).unwrap().resolve();
                self.resolved_arguments.insert(i, resolved);
                Ok(resolved)
            }
            // If the identifier does not need to be resolved, but has already been resolved, then
            // we return the resolved value.
            PTBArg::Identifier(i) if self.resolved_arguments.contains_key(&i) => {
                if ctx.re_resolve() && self.arguments_to_resolve.contains_key(&i) {
                    self.resolve(self.arguments_to_resolve[&i].get().clone(), ctx)
                        .await
                } else {
                    Ok(self.resolved_arguments[&i])
                }
            }
            // Lastly -- look to see if this is an address that has been either declared in scope,
            // or that is coming from an external source (e.g., the keystore).
            PTBArg::Identifier(i) if self.addresses.contains_key(&i) => {
                // We now have a location for this address (which may have come from the keystore
                // so we didnt' have an address for it before), so we tag it with its first usage
                // location put it in the arguments to resolve and resolve away.
                let addr = self.addresses[&i];
                let arg = arg_loc.wrap(PTBArg::Address(NumericalAddress::new(
                    addr.into_bytes(),
                    NumberFormat::Hex,
                )));
                self.arguments_to_resolve
                    .insert(i.clone(), ArgWithHistory::Unresolved(arg.clone()));
                self.resolve(arg_loc.wrap(PTBArg::Identifier(i)), ctx).await
            }
            PTBArg::Address(addr) => {
                let object_id = ObjectID::from_address(addr.into_inner());
                ctx.resolve_object_id(self, arg_loc, object_id).await
            }
            PTBArg::VariableAccess(head, fields) => {
                // Since keystore aliases can contain dots, we need to resolve these/disambiguate
                // them as best as possible here.
                // First: See if structurally we know whether this is a dotted access or a
                // string(alias) containing dot(s). If there is more than one dotted access, or
                // if the field(s) are not all numbers, then we assume it's a alias.
                match self.resolve_variable_access(&head, fields) {
                    sp!(l, ResolvedAccess::DottedString(string)) => {
                        self.resolve(l.wrap(PTBArg::Identifier(string)), ctx).await
                    }
                    sp!(_, ResolvedAccess::ResultAccess(access)) => {
                        match self.resolved_arguments.get(&head.value) {
                            Some(Tx::Argument::Result(u)) => {
                                Ok(Tx::Argument::NestedResult(*u, access))
                            }
                            // Tried to access into a nested result, input, or gascoin
                            Some(
                                x @ (Tx::Argument::NestedResult(..)
                                | Tx::Argument::Input(..)
                                | Tx::Argument::GasCoin),
                            ) => {
                                error!(
                                    arg_loc,
                                    "Tried to access a nested result, input, or gascoin {}: {}",
                                    head.value,
                                    x,
                                );
                            }
                            // Unable to resolve, so now see if we can resolve it to an alias, i.e.,
                            // handle a alias that looks something like `foo.0`
                            None => {
                                let formatted_access = format!("{}.{}", head.value, access);
                                if !self.addresses.contains_key(&formatted_access)
                                    && !self.identifiers.contains_key(&formatted_access)
                                {
                                    match self.did_you_mean_identifier(&head.value) {
                                        Some(similars) => {
                                            error!(
                                                head.span => help: { "{}", similars },
                                                "Tried to access an unresolved identifier: {}", head.value
                                            );
                                        }
                                        None => {
                                            error!(
                                                head.span,
                                                "Tried to access an unresolved identifier: {}",
                                                head.value
                                            );
                                        }
                                    }
                                }
                                self.resolve(
                                    arg_loc.wrap(PTBArg::Identifier(formatted_access.clone())),
                                    ctx,
                                )
                                .await
                            }
                        }
                    }
                }
            }
            // Unable to resolve an identifer to anything at this point -- error and see if we can
            // find a similar identifier to suggest.
            PTBArg::Identifier(i) => match self.did_you_mean_identifier(&i) {
                Some(similars) => {
                    error!(arg_loc => help: { "{}", similars }, "Unresolved identifier: '{}'", i)
                }
                None => error!(arg_loc, "Unresolved identifier: '{}'", i),
            },
        }
    }

    /// Fetch the `SuiObjectData` for an object ID -- this is used for object resolution.
    async fn get_object(&self, object_id: ObjectID, obj_loc: Span) -> PTBResult<SuiObjectData> {
        let res = self
            .reader
            .get_object_with_options(
                object_id,
                SuiObjectDataOptions::new().with_type().with_owner(),
            )
            .await
            .map_err(|e| err!(obj_loc, "{e}"))?
            .into_object()
            .map_err(|e| err!(obj_loc, "{e}"))?;
        Ok(res)
    }

    /// Create a "did you mean" message for an identifier with the context of our different binding
    /// environments.
    fn did_you_mean_identifier(&self, ident: &str) -> Option<String> {
        let did_you_means = find_did_you_means(
            ident,
            self.resolved_arguments
                .keys()
                .chain(self.arguments_to_resolve.keys())
                .chain(self.addresses.keys())
                .map(|x| x.as_str()),
        );
        display_did_you_mean(did_you_means)
    }

    /// Add a single PTB command to the PTB that we are building up. This is the workhorse of it
    /// all.
    async fn handle_command_(
        &mut self,
        cmd_span: Span,
        command: ParsedPTBCommand,
    ) -> PTBResult<()> {
        // let sp!(cmd_span, tok) = &command.name;
        match command {
            ParsedPTBCommand::TransferObjects(obj_args, to_address) => {
                let to_arg = self
                    .resolve(to_address, ToPure::new(TypeTag::Address))
                    .await?;
                let mut transfer_args = vec![];
                for o in obj_args.value.into_iter() {
                    let arg = self.resolve(o, ToObject::default()).await?;
                    transfer_args.push(arg);
                }
                self.last_command = Some(
                    self.ptb
                        .command(Tx::Command::TransferObjects(transfer_args, to_arg)),
                );
            }
            ParsedPTBCommand::Assign(sp!(ident_loc, i), None) => {
                let Some(prev_ptb_arg) = self.last_command.take() else {
                    error!(
                        ident_loc => help: {
                           "This is most likely because the previous command did not \
                           produce a result. E.g., '{ASSIGN}' or '{GAS_BUDGET}' commands do not produce results."

                        },
                        "Cannot assign a value to this variable."
                    );
                };
                self.declare_identifier(i.clone(), ident_loc);
                self.resolved_arguments.insert(i, prev_ptb_arg);
            }
            ParsedPTBCommand::Assign(sp!(ident_loc, i), Some(arg_w_loc)) => {
                self.declare_identifier(i.clone(), ident_loc);
                self.declare_possible_address_binding(i.clone(), &arg_w_loc);
                self.arguments_to_resolve
                    .insert(i, ArgWithHistory::Unresolved(arg_w_loc));
            }
            ParsedPTBCommand::MakeMoveVec(sp!(ty_loc, ty_arg), sp!(_, args)) => {
                let ty_arg = ty_arg
                    .into_type_tag(&resolve_address)
                    .map_err(|e| err!(ty_loc, "{e}"))?;
                let mut vec_args: Vec<Tx::Argument> = vec![];
                if is_primitive_type_tag(&ty_arg) {
                    for arg in args.into_iter() {
                        let arg = self.resolve(arg, ToPure::new(ty_arg.clone())).await?;
                        vec_args.push(arg);
                    }
                } else {
                    for arg in args.into_iter() {
                        let arg = self.resolve(arg, ToObject::default()).await?;
                        vec_args.push(arg);
                    }
                }
                let res = self
                    .ptb
                    .command(Tx::Command::make_move_vec(Some(ty_arg), vec_args));
                self.last_command = Some(res);
            }
            ParsedPTBCommand::SplitCoins(pre_coin, sp!(_, amounts)) => {
                let coin = self.resolve(pre_coin, ToObject::default()).await?;
                let mut args = vec![];
                for arg in amounts.into_iter() {
                    let arg = self.resolve(arg, ToPure::new(TypeTag::U64)).await?;
                    args.push(arg);
                }
                let res = self.ptb.command(Tx::Command::SplitCoins(coin, args));
                self.last_command = Some(res);
            }
            ParsedPTBCommand::MergeCoins(pre_coin, sp!(_, coins)) => {
                let coin = self.resolve(pre_coin, ToObject::default()).await?;
                let mut args = vec![];
                for arg in coins.into_iter() {
                    let arg = self.resolve(arg, ToObject::default()).await?;
                    args.push(arg);
                }
                let res = self.ptb.command(Tx::Command::MergeCoins(coin, args));
                self.last_command = Some(res);
            }
            ParsedPTBCommand::MoveCall(
                sp!(
                    mod_access_loc,
                    PTBModuleAccess {
                        address,
                        module_name,
                        function_name,
                    }
                ),
                in_ty_args,
                args,
            ) => {
                let mut ty_args = vec![];

                if let Some(sp!(ty_loc, in_ty_args)) = in_ty_args {
                    for t in in_ty_args.into_iter() {
                        ty_args.push(
                            t.into_type_tag(&resolve_address)
                                .map_err(|e| err!(ty_loc, "{e}"))?,
                        )
                    }
                }

                let resolved_address = address.value.clone().into_account_address(&|s| {
                    self.addresses.get(s).cloned().or_else(|| resolve_address(s))
                }).map_err(|e| {
                    let e = err!(address.span, "{e}");
                    if let ParsedAddress::Named(name) = address.value {
                        e.with_help(
                            format!("This is most likely because the named address '{name}' is not in scope. \
                                     You can either bind a variable to the address that you want to use or use the address in the command."))
                    } else {
                        e
                    }
                })?;

                let package_id = ObjectID::from_address(resolved_address);
                let package = self.resolve_to_package(package_id, address.span).await?;
                let args = self
                    .resolve_move_call_args(
                        package,
                        &module_name,
                        &function_name,
                        &ty_args,
                        args,
                        mod_access_loc,
                    )
                    .await?;
                let res = self.ptb.command(Tx::Command::move_call(
                    package_id,
                    module_name.value,
                    function_name.value,
                    ty_args,
                    args,
                ));
                self.last_command = Some(res);
            }
            ParsedPTBCommand::Publish(sp!(pkg_loc, package_path)) => {
                let chain_id = self.reader.get_chain_identifier().await.ok();
                let build_config = BuildConfig::default();
                let package_path = Path::new(&package_path);
                let build_config = resolve_lock_file_path(build_config.clone(), Some(package_path))
                    .map_err(|e| err!(pkg_loc, "{e}"))?;
                let previous_id = if let Some(ref chain_id) = chain_id {
                    sui_package_management::set_package_id(
                        package_path,
                        build_config.install_dir.clone(),
                        chain_id,
                        AccountAddress::ZERO,
                    )
                    .map_err(|e| err!(pkg_loc, "{e}"))?
                } else {
                    None
                };
                let compile_result = compile_package(
                    self.reader,
                    build_config.clone(),
                    package_path,
                    false, /* with_unpublished_dependencies */
                    false, /* skip_dependency_verification */
                )
                .await;
                // Restore original ID, then check result.
                if let (Some(chain_id), Some(previous_id)) = (chain_id, previous_id) {
                    let _ = sui_package_management::set_package_id(
                        package_path,
                        build_config.install_dir.clone(),
                        &chain_id,
                        previous_id,
                    )
                    .map_err(|e| err!(pkg_loc, "{e}"))?;
                }
                let (dependencies, compiled_modules, _, _) =
                    compile_result.map_err(|e| err!(pkg_loc, "{e}"))?;

                let res = self.ptb.publish_upgradeable(
                    compiled_modules,
                    dependencies.published.into_values().collect(),
                );
                self.last_command = Some(res);
            }
            // Update this command to not do as many things. It should result in a single command.
            ParsedPTBCommand::Upgrade(sp!(path_loc, package_path), mut arg) => {
                if let sp!(loc, PTBArg::Identifier(id)) = arg {
                    arg = self
                        .arguments_to_resolve
                        .get(&id)
                        .and_then(|x| x.get_unresolved())
                        .ok_or_else(|| err!(loc, "Unable to find object ID argument"))?
                        .clone();
                }
                let (cap_loc, upgrade_cap_id) = match arg {
                    sp!(loc, PTBArg::Address(id)) => (loc, id),
                    sp!(loc, _) => {
                        error!(loc, "Expected upgrade capability object ID");
                    }
                };

                let upgrade_cap_arg = self
                    .resolve(
                        cap_loc.wrap(PTBArg::Address(upgrade_cap_id)),
                        ToObject::default(),
                    )
                    .await?;

                let chain_id = self.reader.get_chain_identifier().await.ok();
                let build_config = BuildConfig::default();
                let package_path = Path::new(&package_path);
                let build_config = resolve_lock_file_path(build_config.clone(), Some(package_path))
                    .map_err(|e| err!(path_loc, "{e}"))?;
                let previous_id = if let Some(ref chain_id) = chain_id {
                    sui_package_management::set_package_id(
                        package_path,
                        build_config.install_dir.clone(),
                        chain_id,
                        AccountAddress::ZERO,
                    )
                    .map_err(|e| err!(path_loc, "{e}"))?
                } else {
                    None
                };
                let upgrade_result = upgrade_package(
                    self.reader,
                    build_config.clone(),
                    package_path,
                    ObjectID::from_address(upgrade_cap_id.into_inner()),
                    false, /* with_unpublished_dependencies */
                    false, /* skip_dependency_verification */
                    None,
                )
                .await;
                // Restore original ID, then check result.
                if let (Some(chain_id), Some(previous_id)) = (chain_id, previous_id) {
                    let _ = sui_package_management::set_package_id(
                        package_path,
                        build_config.install_dir.clone(),
                        &chain_id,
                        previous_id,
                    )
                    .map_err(|e| err!(path_loc, "{e}"))?;
                }
                let (package_id, compiled_modules, dependencies, package_digest, upgrade_policy, _) =
                    upgrade_result.map_err(|e| err!(path_loc, "{e}"))?;

                let upgrade_arg = self
                    .ptb
                    .pure(upgrade_policy)
                    .map_err(|e| err!(cmd_span, "{e}"))?;
                let digest_arg = self
                    .ptb
                    // .to_vec() is necessary to get the length prefix
                    .pure(package_digest.to_vec())
                    .map_err(|e| err!(cmd_span, "{e}"))?;
                let upgrade_ticket = self.ptb.command(Tx::Command::move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    ident_str!("package").to_owned(),
                    ident_str!("authorize_upgrade").to_owned(),
                    vec![],
                    vec![upgrade_cap_arg, upgrade_arg, digest_arg],
                ));
                let upgrade_receipt = self.ptb.upgrade(
                    package_id,
                    upgrade_ticket,
                    dependencies.published.into_values().collect(),
                    compiled_modules,
                );
                let res = self.ptb.command(Tx::Command::move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    ident_str!("package").to_owned(),
                    ident_str!("commit_upgrade").to_owned(),
                    vec![],
                    vec![upgrade_cap_arg, upgrade_receipt],
                ));
                self.last_command = Some(res);
            }
            ParsedPTBCommand::WarnShadows => {}
            ParsedPTBCommand::Preview => {}
        }
        Ok(())
    }
}

// ===========================================================================
// Helper methods
// ===========================================================================
pub fn to_ordinal_contraction(num: usize) -> String {
    let suffix = match num % 100 {
        // exceptions
        11..=13 => "th",
        _ => match num % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{}{}", num, suffix)
}

pub(crate) fn find_did_you_means<'a>(
    needle: &str,
    haystack: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let mut results = Vec::new();
    let mut best_distance = usize::MAX;

    for item in haystack {
        let distance = edit_distance(needle, item);

        match distance.cmp(&best_distance) {
            std::cmp::Ordering::Less => {
                best_distance = distance;
                results.clear();
                results.push(item);
            }
            std::cmp::Ordering::Equal => {
                results.push(item);
            }
            std::cmp::Ordering::Greater => {}
        }
    }

    results
}

pub(crate) fn display_did_you_mean<S: AsRef<str> + std::fmt::Display>(
    possibles: Vec<S>,
) -> Option<String> {
    if possibles.is_empty() {
        return None;
    }

    let mut strs = vec![];

    let preposition = if possibles.len() == 1 {
        "Did you mean "
    } else {
        "Did you mean one of "
    };

    let len = possibles.len();
    for (i, possible) in possibles.into_iter().enumerate() {
        if i == len - 1 && len > 1 {
            strs.push(format!("or '{}'", possible));
        } else {
            strs.push(format!("'{}'", possible));
        }
    }

    Some(format!("{preposition}{}?", strs.join(", ")))
}

// This lint is disabled because it's not good and doesn't look at what you're actually
// iterating over. This seems to be a common problem with this lint.
// See e.g., https://github.com/rust-lang/rust-clippy/issues/6075
#[allow(clippy::needless_range_loop)]
fn edit_distance(a: &str, b: &str) -> usize {
    let mut cache = vec![vec![0; b.len() + 1]; a.len() + 1];

    for i in 0..=a.len() {
        cache[i][0] = i;
    }

    for j in 0..=b.len() {
        cache[0][j] = j;
    }

    for (i, char_a) in a.chars().enumerate().map(|(i, c)| (i + 1, c)) {
        for (j, char_b) in b.chars().enumerate().map(|(j, c)| (j + 1, c)) {
            if char_a == char_b {
                cache[i][j] = cache[i - 1][j - 1];
            } else {
                let insert = cache[i][j - 1];
                let delete = cache[i - 1][j];
                let replace = cache[i - 1][j - 1];

                cache[i][j] = 1 + std::cmp::min(insert, std::cmp::min(delete, replace));
            }
        }
    }

    cache[a.len()][b.len()]
}
