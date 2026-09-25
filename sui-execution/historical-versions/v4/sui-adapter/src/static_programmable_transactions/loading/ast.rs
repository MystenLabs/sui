// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    gas_charger::GasPayment,
    static_programmable_transactions::linkage::resolved_linkage::{
        ExecutableLinkage, ResolvedLinkage,
    },
};
use indexmap::IndexSet;
use move_binary_format::{
    CompiledModule,
    file_format::{AbilitySet, CodeOffset, FunctionDefinitionIndex, Visibility},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    u256::U256,
};
use std::{collections::BTreeSet, rc::Rc};
use sui_types::{
    Identifier, TypeTag,
    base_types::{ObjectID, ObjectRef, RESOLVED_TX_CONTEXT, SequenceNumber, TxContextKind},
    object::ObjectPermissions,
};
use sui_verifier::INIT_FN_NAME;

//**************************************************************************************************
// AST Nodes
//**************************************************************************************************

#[derive(Debug)]
pub struct Transaction {
    pub gas_payment: Option<GasPayment>,
    pub inputs: Inputs,
    /// Original number of commands in the transaction. After typing, Spanned indices in the AST
    /// should be < `original_command_len`
    pub original_command_len: usize,
    pub commands: Commands,
    pub unified_linkage: Option<ExecutableLinkage>,
}

pub type Inputs = Vec<(InputArg, InputType)>;

pub type Commands = Vec<Command>;

#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
pub enum InputArg {
    Pure(Vec<u8>),
    Receiving(ObjectRef),
    Object(ObjectArg),
    FundsWithdrawal(FundsWithdrawalArg),
}

#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
pub enum ObjectArgKind {
    ImmObject(ObjectRef),
    OwnedObject(ObjectRef),
    ConsensusObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
    },
}

#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
pub struct ObjectArg {
    pub kind: ObjectArgKind,
    /// Permissions, potentially refined/limited based on the input argument. For example if a
    /// shared object is used but marked as read-only, the permissions would be refined to being
    /// _only_ immutable usage.
    pub refined_permissions: ObjectPermissions,
}

#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
pub struct FundsWithdrawalArg {
    // if true, it was from a compatibility object input, not a intentional withdrawal argument
    pub from_compatibility_object: bool,
    /// The full type.
    /// Either `sui::funds_accumulator::Withdrawal<T>` for a direct source, or
    /// `sui::allowance::AllowanceWithdrawal<T>` for an allowance source
    pub ty: Type,
    pub source: WithdrawalSource,
    /// This amount is verified to be <= the max for the type described by the `T` in `ty`
    pub amount: U256,
}

#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
pub enum WithdrawalSource {
    /// A `sui::funds_accumulator::Withdrawal` from the sender/sponsor
    Direct { owner: AccountAddress },
    /// An `sui::allowance::AllowanceWithdrawal` permissioned by the `Allowance` object `id`
    Allowance {
        funder: AccountAddress,
        id: ObjectID,
    },
}

impl WithdrawalSource {
    /// The account the withdrawal debits.
    pub fn source_account(&self) -> AccountAddress {
        match self {
            Self::Direct { owner } => *owner,
            Self::Allowance { funder, .. } => *funder,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(Rc<Vector>),
    Datatype(Rc<Datatype>),
    Reference(/* is mut */ bool, Rc<Type>),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Vector {
    pub abilities: AbilitySet,
    pub element_type: Type,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Datatype {
    pub abilities: AbilitySet,
    pub module: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
}

#[derive(Debug, Clone)]
pub enum InputType {
    Bytes,
    Fixed(Type),
}

#[derive(Debug)]
pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(/* T for vector<T> */ Option<Type>, Vec<Argument>),
    Publish(PackagePayload, Vec<ObjectID>, ResolvedLinkage),
    Upgrade(
        PackagePayload,
        Vec<ObjectID>,
        ObjectID,
        Argument,
        ResolvedLinkage,
    ),
}

#[derive(Debug, Clone)]
pub enum PackagePayload {
    Serialized(Vec<Vec<u8>>),
    Deserialized(DeserializedPackage),
}

// A Deserialized but not yet verified package created as part of loading.
#[derive(Debug, Clone)]
pub struct DeserializedPackage {
    // NB: Modules are deserialized but not yet verified. They _are_ bounds checked though.
    pub deserialized_modules: Vec<CompiledModule>,
    // Sum of the sizes of all modules in (serialized) bytes, used for metering
    pub total_bytes: usize,
    // The computed digest of the package --
    // `MovePackage::compute_digest_for_modules_and_deps` with `hash_modules` set to `true`.
    pub computed_digest: [u8; 32],
    // Names of the modules in this package that define a function named `init`.
    pub modules_with_init: BTreeSet<Identifier>,
}

impl DeserializedPackage {
    pub fn new(
        deserialized_modules: Vec<CompiledModule>,
        total_bytes: usize,
        computed_digest: [u8; 32],
    ) -> Self {
        let modules_with_init = deserialized_modules
            .iter()
            .filter(|module| module_has_init(module))
            .map(|module| module.identifier_at(module.self_handle().name).to_owned())
            .collect();
        Self {
            deserialized_modules,
            total_bytes,
            computed_digest,
            modules_with_init,
        }
    }

    /// Returns true if this package defines any modules with function that could be a possible
    /// `init` function.
    pub fn has_potential_init(&self) -> bool {
        !self.modules_with_init.is_empty()
    }
}

/// Whether `module` defines a function named `init`.
///
/// NB: we presuppose that a function named `init` is the module's initializer. If it does not
/// conform to the `init` signature requirements the entry points verifier rejects the publish
/// later, failing the transaction as a whole.
pub(crate) fn module_has_init(module: &CompiledModule) -> bool {
    module.function_defs().iter().any(|func_def| {
        let handle = module.function_handle_at(func_def.function);
        module.identifier_at(handle.name) == INIT_FN_NAME
    })
}

#[derive(Debug)]
pub struct LoadedFunctionInstantiation {
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}

#[derive(Debug)]
pub struct LoadedFunction {
    pub version_mid: ModuleId,
    pub original_mid: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
    pub signature: LoadedFunctionInstantiation,
    pub linkage: ExecutableLinkage,
    pub instruction_length: CodeOffset,
    pub definition_index: FunctionDefinitionIndex,
    pub visibility: Visibility,
    pub is_entry: bool,
    pub is_native: bool,
}

#[derive(Debug)]
pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

pub use sui_types::transaction::Argument;

//**************************************************************************************************
// impl
//**************************************************************************************************

impl ObjectArg {
    pub fn id(&self) -> ObjectID {
        self.kind.id()
    }
}

impl ObjectArgKind {
    pub fn id(&self) -> ObjectID {
        match self {
            Self::ImmObject(oref) | Self::OwnedObject(oref) => oref.0,
            Self::ConsensusObject { id, .. } => *id,
        }
    }
}

impl Type {
    pub fn abilities(&self) -> AbilitySet {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => AbilitySet::PRIMITIVES,
            Type::Signer => AbilitySet::SIGNER,
            Type::Reference(_, _) => AbilitySet::REFERENCES,
            Type::Vector(v) => v.abilities,
            Type::Datatype(dt) => dt.abilities,
        }
    }

    pub fn is_tx_context(&self) -> TxContextKind {
        let (is_mut, inner) = match self {
            Type::Reference(is_mut, inner) => (*is_mut, inner),
            _ => return TxContextKind::None,
        };
        let Type::Datatype(dt) = &**inner else {
            return TxContextKind::None;
        };
        if dt.qualified_ident() == RESOLVED_TX_CONTEXT {
            if is_mut {
                TxContextKind::Mutable
            } else {
                TxContextKind::Immutable
            }
        } else {
            TxContextKind::None
        }
    }

    /// Is this the `TxContext` datatype itself, not behind a reference?
    pub fn is_tx_context_by_value(&self) -> bool {
        matches!(self, Type::Datatype(dt) if dt.qualified_ident() == RESOLVED_TX_CONTEXT)
    }

    pub fn all_addresses(&self) -> IndexSet<AccountAddress> {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address
            | Type::Signer => IndexSet::new(),
            Type::Vector(v) => v.element_type.all_addresses(),
            Type::Reference(_, inner) => inner.all_addresses(),
            Type::Datatype(dt) => dt.all_addresses(),
        }
    }

    pub fn node_count(&self) -> u64 {
        use Type::*;
        let mut total = 0u64;
        let mut stack = vec![self];

        while let Some(ty) = stack.pop() {
            total = total.saturating_add(1);
            match ty {
                Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer => {}
                Vector(v) => stack.push(&v.element_type),
                Reference(_, inner) => stack.push(inner),
                Datatype(dt) => {
                    stack.extend(&dt.type_arguments);
                }
            }
        }

        total
    }

    pub fn is_reference(&self) -> bool {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address
            | Type::Signer
            | Type::Vector(_)
            | Type::Datatype(_) => false,
            Type::Reference(_, _) => true,
        }
    }
}

impl Datatype {
    pub fn qualified_ident(&self) -> (&AccountAddress, &IdentStr, &IdentStr) {
        (
            self.module.address(),
            self.module.name(),
            self.name.as_ident_str(),
        )
    }

    pub fn all_addresses(&self) -> IndexSet<AccountAddress> {
        let mut addresses = IndexSet::new();
        addresses.insert(*self.module.address());
        for arg in &self.type_arguments {
            addresses.extend(arg.all_addresses());
        }
        addresses
    }
}

impl Command {
    pub fn arguments_mut(&mut self) -> Box<dyn Iterator<Item = &mut Argument> + '_> {
        match self {
            Command::MoveCall(mc) => Box::new(mc.arguments.iter_mut()),
            Command::TransferObjects(objs, recipient) => {
                Box::new(objs.iter_mut().chain(std::iter::once(recipient)))
            }
            Command::SplitCoins(coin, amounts) => {
                Box::new(std::iter::once(coin).chain(amounts.iter_mut()))
            }
            Command::MergeCoins(coin, coins) => {
                Box::new(std::iter::once(coin).chain(coins.iter_mut()))
            }
            Command::MakeMoveVec(_, elements) => Box::new(elements.iter_mut()),
            Command::Publish(_, _, _) => Box::new(std::iter::empty()),
            Command::Upgrade(_, _, _, obj, _) => Box::new(std::iter::once(obj)),
        }
    }

    pub fn arguments(&self) -> Box<dyn Iterator<Item = &Argument> + '_> {
        match self {
            Command::MoveCall(mc) => Box::new(mc.arguments.iter()),
            Command::TransferObjects(objs, recipient) => {
                Box::new(objs.iter().chain(std::iter::once(recipient)))
            }
            Command::SplitCoins(coin, amounts) => {
                Box::new(std::iter::once(coin).chain(amounts.iter()))
            }
            Command::MergeCoins(coin, coins) => Box::new(std::iter::once(coin).chain(coins.iter())),
            Command::MakeMoveVec(_, elements) => Box::new(elements.iter()),
            Command::Publish(_, _, _) => Box::new(std::iter::empty()),
            Command::Upgrade(_, _, _, obj, _) => Box::new(std::iter::once(obj)),
        }
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TryFrom<Type> for TypeTag {
    type Error = &'static str;
    fn try_from(ty: Type) -> Result<Self, Self::Error> {
        Ok(match ty {
            Type::Bool => TypeTag::Bool,
            Type::U8 => TypeTag::U8,
            Type::U16 => TypeTag::U16,
            Type::U32 => TypeTag::U32,
            Type::U64 => TypeTag::U64,
            Type::U128 => TypeTag::U128,
            Type::U256 => TypeTag::U256,
            Type::Address => TypeTag::Address,
            Type::Signer => TypeTag::Signer,
            Type::Vector(inner) => {
                let Vector { element_type, .. } = &*inner;
                TypeTag::Vector(Box::new(element_type.clone().try_into()?))
            }
            Type::Datatype(dt) => {
                let dt: &Datatype = &dt;
                TypeTag::Struct(Box::new(dt.try_into()?))
            }
            Type::Reference(_, _) => return Err("unexpected reference type"),
        })
    }
}

impl TryFrom<&Datatype> for StructTag {
    type Error = &'static str;

    fn try_from(dt: &Datatype) -> Result<Self, Self::Error> {
        let Datatype {
            module,
            name,
            type_arguments,
            ..
        } = dt;
        Ok(StructTag {
            address: *module.address(),
            module: module.name().to_owned(),
            name: name.to_owned(),
            type_params: type_arguments
                .iter()
                .map(|t| t.clone().try_into())
                .collect::<Result<Vec<TypeTag>, _>>()?,
        })
    }
}
