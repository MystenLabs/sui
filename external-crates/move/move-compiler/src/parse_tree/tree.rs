// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::Diagnostic,
    parser::{comments::Comment, lexer::Tok},
};

use move_ir_types::location::{Loc, Spanned};

pub type ParseTree = Spanned<ParseTree_>;

#[derive(Clone)]
pub enum ParseTree_ {
    Module {
        module_keyword: ParsedToken,
        name: Box<ParseTree>,
        body: Box<ParseTree>,
    },
    Script {
        body: Box<ParseTree>,
    },
    AddressBlock {
        address_keywork: ParsedToken,
        address: ParsedToken,
        modules: Box<ParseTree>,
    },
    // Attribute = "#" "[" Comma<Attribute> "]"
    Attribute {
        hash_sign: ParsedToken,
        attrs: Box<ParseTree>,
    },

    // UseDecl = "use" <ModuleIdent> <UseAlias> ";"
    UseDeclAlias {
        use_keyword: ParsedToken,
        mident: Box<ParseTree>,
        use_alias: Option<Box<ParseTree>>,
        scolon: ParsedToken,
    },

    // UseDecl =
    //     "use" <ModuleIdent> :: <UseMember> ";" |
    //     "use" <ModuleIdent> :: "{" Comma<UseMember> "}" ";"
    UseDeclMember {
        use_keyword: ParsedToken,
        mident: Box<ParseTree>,
        dcolon: ParsedToken,
        use_member: Box<ParseTree>,
        scolon: ParsedToken,
    },

    // UseMember = <Identifier> <UseAlias>
    UseMember {
        name: ParsedToken,
        use_alias: Box<ParseTree>,
    },

    // UseAlias = ("as" <Identifier>)?
    UseAlias {
        as_keyword: ParsedToken,
        alias: ParsedToken,
    },

    //      Attribute =
    //          <Identifier>
    //          | <Identifier> "=" <AttributeValue>
    //          | <Identifier> "(" Comma<Attribute> ")"
    AttributeName {
        name: ParsedToken,
    },
    AttributeAssigned {
        name: ParsedToken,
        eq_sign: ParsedToken,
        value: Box<ParseTree>,
    },
    AttributeParameterized {
        name: ParsedToken,
        lparen: ParsedToken,
        attributes: Vec<ParseTree>,
        rparen: ParsedToken,
    },

    // NameAccessChain = <LeadingNameAccess> ( "::" <Identifier> ( "::" <Identifier> )? )?
    NameAccessChainOne {
        name: ParsedToken,
    },
    NameAccessChainTwo {
        name1: ParsedToken,
        dcolon: ParsedToken,
        name2: ParsedToken,
    },
    NameAccessChainThree {
        name1: ParsedToken,
        dcolon1: ParsedToken,
        name2: ParsedToken,
        dcolon2: ParsedToken,
        name3: ParsedToken,
    },

    Identifier {
        name: ParsedToken,
    },
    SeparatedList {
        separator: Option<ParsedToken>,
        elements: Vec<Box<ParseTree>>,
    },
    // a block of ParseTree elements enclosed in different types of braces/parens, for example
    // (...), [...], {...}
    CodeBlock {
        lbrace: ParsedToken,
        elements: Vec<Box<ParseTree>>,
        rbrace: ParsedToken,
    },
}

#[derive(Clone)]
pub struct ParsedToken {
    kind: Tok,
    range: Loc,
    contents: Box<str>,
    leading_comments: Vec<Comment>,
    diags: Vec<Diagnostic>,
}
