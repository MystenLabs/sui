// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Contains AST definitions for the specification language fragments of the Move language.
//! Note that in this crate, specs are represented in AST form, whereas code is represented
//! as bytecodes. Therefore we do not need an AST for the Move code itself.

use std::{
    fmt,
    fmt::{Debug, Error, Formatter},
    hash::Hash,
};

use num::{BigInt, BigUint, Num};
use once_cell::sync::Lazy;

use crate::{
    model::NodeId,
    symbol::{Symbol, SymbolPool},
};

const MAX_ADDR_STRING: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// A type alias for temporaries. Those are locals used in bytecode.
pub type TempIndex = usize;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum TraceKind {
    /// A user level TRACE(..) in the source.
    User,
    /// An automatically generated trace
    Auto,
    /// A trace for a sub-expression of an assert or assume. The location of a
    /// Call(.., Trace(SubAuto)) expression identifies the context of the assume or assert.
    /// A backend may print those traces only if the assertion failed.
    SubAuto,
}

// =================================================================================================
/// # Attributes

#[derive(Debug, Clone)]
pub enum AttributeValue {
    Value(NodeId, Value),
    Name(NodeId, Option<ModuleName>, Symbol),
}

#[derive(Debug, Clone)]
pub enum Attribute {
    Apply(NodeId, Symbol, Vec<Attribute>),
    Assign(NodeId, Symbol, AttributeValue),
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Value {
    Address(BigUint),
    Number(BigInt),
    Bool(bool),
    ByteArray(Vec<u8>),
    AddressArray(Vec<BigUint>), // TODO: merge AddressArray to Vector type in the future
    Vector(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Value::Address(address) => write!(f, "{:x}", address),
            Value::Number(int) => write!(f, "{}", int),
            Value::Bool(b) => write!(f, "{}", b),
            // TODO(tzakian): Figure out a better story for byte array displays
            Value::ByteArray(bytes) => write!(f, "{:?}", bytes),
            Value::AddressArray(array) => write!(f, "{:?}", array),
            Value::Vector(array) => write!(f, "{:?}", array),
        }
    }
}

// =================================================================================================
/// # Names

/// Represents a module name, consisting of address and name.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct ModuleName(BigUint, Symbol);

impl ModuleName {
    pub fn new(addr: BigUint, name: Symbol) -> ModuleName {
        ModuleName(addr, name)
    }

    pub fn from_address_bytes_and_name(
        addr: move_compiler::shared::NumericalAddress,
        name: Symbol,
    ) -> ModuleName {
        ModuleName(BigUint::from_bytes_be(&addr.into_bytes()), name)
    }

    pub fn from_str(mut addr: &str, name: Symbol) -> ModuleName {
        if addr.starts_with("0x") {
            addr = &addr[2..];
        }
        let bi = BigUint::from_str_radix(addr, 16).expect("valid hex");
        ModuleName(bi, name)
    }

    pub fn addr(&self) -> &BigUint {
        &self.0
    }

    pub fn name(&self) -> Symbol {
        self.1
    }

    /// Determine whether this is a script. The move-compiler infrastructure uses MAX_ADDR
    /// for pseudo modules created from scripts, so use this address to check.
    pub fn is_script(&self) -> bool {
        static MAX_ADDR: Lazy<BigUint> =
            Lazy::new(|| BigUint::from_str_radix(MAX_ADDR_STRING, 16).expect("valid hex"));
        self.0 == *MAX_ADDR
    }
}

impl ModuleName {
    /// Creates a value implementing the Display trait which shows this name,
    /// excluding address.
    pub fn display<'a>(&'a self, pool: &'a SymbolPool) -> ModuleNameDisplay<'a> {
        ModuleNameDisplay {
            name: self,
            pool,
            with_address: false,
        }
    }

    /// Creates a value implementing the Display trait which shows this name,
    /// including address.
    pub fn display_full<'a>(&'a self, pool: &'a SymbolPool) -> ModuleNameDisplay<'a> {
        ModuleNameDisplay {
            name: self,
            pool,
            with_address: true,
        }
    }
}

/// A helper to support module names in formatting.
pub struct ModuleNameDisplay<'a> {
    name: &'a ModuleName,
    pool: &'a SymbolPool,
    with_address: bool,
}

impl<'a> fmt::Display for ModuleNameDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        if self.with_address && !self.name.is_script() {
            write!(f, "0x{}::", self.name.0.to_str_radix(16))?;
        }
        write!(f, "{}", self.name.1.display(self.pool))?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct QualifiedSymbol {
    pub module_name: ModuleName,
    pub symbol: Symbol,
}

impl QualifiedSymbol {
    /// Creates a value implementing the Display trait which shows this symbol,
    /// including module name but excluding address.
    pub fn display<'a>(&'a self, pool: &'a SymbolPool) -> QualifiedSymbolDisplay<'a> {
        QualifiedSymbolDisplay {
            sym: self,
            pool,
            with_module: true,
            with_address: false,
        }
    }

    /// Creates a value implementing the Display trait which shows this qualified symbol,
    /// excluding module name.
    pub fn display_simple<'a>(&'a self, pool: &'a SymbolPool) -> QualifiedSymbolDisplay<'a> {
        QualifiedSymbolDisplay {
            sym: self,
            pool,
            with_module: false,
            with_address: false,
        }
    }

    /// Creates a value implementing the Display trait which shows this symbol,
    /// including module name with address.
    pub fn display_full<'a>(&'a self, pool: &'a SymbolPool) -> QualifiedSymbolDisplay<'a> {
        QualifiedSymbolDisplay {
            sym: self,
            pool,
            with_module: true,
            with_address: true,
        }
    }
}

/// A helper to support qualified symbols in formatting.
pub struct QualifiedSymbolDisplay<'a> {
    sym: &'a QualifiedSymbol,
    pool: &'a SymbolPool,
    with_module: bool,
    with_address: bool,
}

impl<'a> fmt::Display for QualifiedSymbolDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        if self.with_module {
            write!(
                f,
                "{}::",
                if self.with_address {
                    self.sym.module_name.display_full(self.pool)
                } else {
                    self.sym.module_name.display(self.pool)
                }
            )?;
        }
        write!(f, "{}", self.sym.symbol.display(self.pool))?;
        Ok(())
    }
}
