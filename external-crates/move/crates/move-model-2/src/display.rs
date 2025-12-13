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
        use N::TypeInner as TI;
        match self.0.inner() {
            TI::Unit => write!(f, "()"),
            TI::Ref(false, inner) => write!(f, "&{}", type_(inner)),
            TI::Ref(true, inner) => write!(f, "&mut {}", type_(inner)),
            TI::Param(tp) => write!(f, "{}", tp.user_specified_name),
            TI::Apply(_, sp!(_, tn), targs) => match tn {
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
            TI::Fun(targs, tret) => {
                write!(f, "|")?;
                for (i, t) in targs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", type_(t))?;
                }
                write!(f, "| -> {}", type_(tret))
            }
            TI::Var(_) | TI::Anything | TI::Void | TI::UnresolvedError => {
                write!(f, "_")
            }
        }
    }
}
