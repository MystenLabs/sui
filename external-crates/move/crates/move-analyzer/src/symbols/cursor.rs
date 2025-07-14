// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains definitions and implementations supporting the notion of cursor
//! which gets compuuted during symbolication process to contain information about the
//! position of the cursor in the source code,

use std::fmt;

use move_compiler::{expansion::ast::ModuleIdent, parser::ast as P, shared::Name};
use move_ir_types::location::*;

#[derive(Clone, Debug)]
pub struct CursorContext {
    /// Set during typing analysis
    pub module: Option<ModuleIdent>,
    /// Set during typing analysis
    pub defn_name: Option<CursorDefinition>,
    // TODO: consider making this a vector to hold the whole chain upward
    /// Set during parsing analysis
    pub position: CursorPosition,
    /// Location provided for the cursor
    pub loc: Loc,
}

#[derive(Clone, Debug, Copy)]
pub enum ChainCompletionKind {
    Type,
    Function,
    All,
}

#[derive(Clone, Debug)]
pub struct ChainInfo {
    pub chain: P::NameAccessChain,
    pub kind: ChainCompletionKind,
    pub inside_use: bool,
}

#[derive(Clone, Debug)]
pub enum CursorPosition {
    Exp(P::Exp),
    SeqItem(P::SequenceItem),
    Binding(P::Bind),
    Type(P::Type),
    FieldDefn(P::Field),
    Parameter(P::Var),
    DefName,
    Attribute(P::AttributeValue),
    Use(Spanned<P::Use>),
    MatchPattern(P::MatchPattern),
    Unknown,
    // FIXME: These two are currently unused because these forms don't have enough location
    // recorded on them during parsing.
    DatatypeTypeParameter(P::DatatypeTypeParameter),
    FunctionTypeParameter((Name, Vec<P::Ability>)),
}

#[derive(Clone, Debug)]
pub enum CursorDefinition {
    Function(P::FunctionName),
    Constant(P::ConstantName),
    Struct(P::DatatypeName),
    Enum(P::DatatypeName),
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl ChainInfo {
    pub fn new(chain: P::NameAccessChain, kind: ChainCompletionKind, inside_use: bool) -> Self {
        Self {
            chain,
            kind,
            inside_use,
        }
    }
}

impl CursorContext {
    pub fn new(loc: Loc) -> Self {
        CursorContext {
            module: None,
            defn_name: None,
            position: CursorPosition::Unknown,
            loc,
        }
    }

    /// Returns access chain for a match pattern, if any
    fn find_access_chain_in_match_pattern(&self, p: &P::MatchPattern_) -> Option<ChainInfo> {
        use ChainCompletionKind as CT;
        use P::MatchPattern_ as MP;
        match p {
            MP::PositionalConstructor(chain, _) => {
                Some(ChainInfo::new(chain.clone(), CT::Type, false))
            }
            MP::FieldConstructor(chain, _) => Some(ChainInfo::new(chain.clone(), CT::Type, false)),
            MP::Name(_, chain) => Some(ChainInfo::new(chain.clone(), CT::All, false)),
            MP::Literal(_) | MP::Or(..) | MP::At(..) => None,
        }
    }

    /// Returns access chain at cursor position (if any) along with the information of what the chain's
    /// auto-completed target kind should be, and weather it is part of the use statement.
    pub fn find_access_chain(&self) -> Option<ChainInfo> {
        use ChainCompletionKind as CT;
        use CursorPosition as CP;
        match &self.position {
            CP::Exp(sp!(_, exp)) => match exp {
                P::Exp_::Name(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::All, false));
                }
                P::Exp_::Call(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Function, false));
                }
                P::Exp_::Pack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Type, false));
                }
                _ => (),
            },
            CP::Binding(sp!(_, bind)) => match bind {
                P::Bind_::Unpack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false));
                }
                _ => (),
            },
            CP::Type(sp!(_, ty)) => match ty {
                P::Type_::Apply(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false));
                }
                _ => (),
            },
            CP::Attribute(attr_val) => match &attr_val.value {
                P::AttributeValue_::ModuleAccess(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::All, false));
                }
                _ => (),
            },
            CP::Use(sp!(_, P::Use::Fun { function, ty, .. })) => {
                if function.loc.contains(&self.loc) {
                    return Some(ChainInfo::new(*(function.clone()), CT::Function, true));
                }
                if ty.loc.contains(&self.loc) {
                    return Some(ChainInfo::new(*(ty.clone()), CT::Type, true));
                }
            }
            CP::MatchPattern(sp!(_, p)) => return self.find_access_chain_in_match_pattern(p),
            _ => (),
        };
        None
    }

    /// Returns use declaration at cursor position (if any).
    pub fn find_use_decl(&self) -> Option<P::Use> {
        if let CursorPosition::Use(use_) = &self.position {
            return Some(use_.value.clone());
        }
        None
    }
}

impl fmt::Display for CursorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let CursorContext {
            module,
            defn_name,
            position,
            loc: _,
        } = self;
        writeln!(f, "cursor info:")?;
        write!(f, "- module: ")?;
        match module {
            Some(mident) => writeln!(f, "{mident}"),
            None => writeln!(f, "None"),
        }?;
        write!(f, "- definition: ")?;
        match defn_name {
            Some(defn) => match defn {
                CursorDefinition::Function(name) => writeln!(f, "function {name}"),
                CursorDefinition::Constant(name) => writeln!(f, "constant {name}"),
                CursorDefinition::Struct(name) => writeln!(f, "struct {name}"),
                CursorDefinition::Enum(name) => writeln!(f, "enum {name}"),
            },
            None => writeln!(f, "None"),
        }?;
        write!(f, "- position: ")?;
        match position {
            CursorPosition::Attribute(value) => {
                writeln!(f, "attribute value")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Use(value) => {
                writeln!(f, "use value")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::DefName => {
                writeln!(f, "defn name")?;
            }
            CursorPosition::Unknown => {
                writeln!(f, "unknown")?;
            }
            CursorPosition::Exp(value) => {
                writeln!(f, "exp")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::SeqItem(value) => {
                writeln!(f, "seq item")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Binding(value) => {
                writeln!(f, "binder")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Type(value) => {
                writeln!(f, "type")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::FieldDefn(value) => {
                writeln!(f, "field")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Parameter(value) => {
                writeln!(f, "parameter")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::MatchPattern(value) => {
                writeln!(f, "match pattern")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::DatatypeTypeParameter(value) => {
                writeln!(f, "datatype type param")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::FunctionTypeParameter(value) => {
                writeln!(f, "fun type param")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
        }
        Ok(())
    }
}
