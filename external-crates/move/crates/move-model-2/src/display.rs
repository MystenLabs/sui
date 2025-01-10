// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use core::fmt;

use move_compiler::naming::ast as N;

pub struct Type<'a>(&'a N::Type_);

pub fn type_(t: &N::Type) -> Type<'_> {
    Type(&t.value)
}

impl fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            N::Type_::Unit => write!(f, "()"),
            N::Type_::Ref(false, inner) => write!(f, "&{}", type_(inner)),
            N::Type_::Ref(true, inner) => write!(f, "&mut {}", type_(inner)),
            N::Type_::Param(tp) => write!(f, "{}", tp.user_specified_name),
            N::Type_::Apply(_, sp!(_, tn), targs) => match tn {
                N::TypeName_::Multiple(_) => {
                    debug_assert!(targs.len() > 1);
                    write!(f, "(")?;
                    for (i, t) in targs.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", type_(t))?;
                    }
                    write!(f, ")")
                }
                N::TypeName_::ModuleType(_, _) | N::TypeName_::Builtin(_) => {
                    write!(f, "{tn}")?;
                    if !targs.is_empty() {
                        write!(f, "<")?;
                        for (i, t) in targs.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}", type_(t))?;
                        }
                        write!(f, ">")?;
                    }
                    Ok(())
                }
            },
            N::Type_::Fun(targs, tret) => {
                write!(f, "|")?;
                for (i, t) in targs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", type_(t))?;
                }
                write!(f, "| -> {}", type_(tret))
            }
            N::Type_::Var(_) | N::Type_::Anything | N::Type_::UnresolvedError => write!(f, "_"),
        }
    }
}
