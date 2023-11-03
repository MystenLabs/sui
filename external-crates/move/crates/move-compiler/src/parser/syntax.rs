// Copyright (c) The Diem Core Contributors Copyright (c) The Move Contributors SPDX-License-Identifier: Apache-2.0 In the informal grammar comments in this file, Comma<T> is shorthand for:
//      (<T> ",")* <T>?
// Note that this allows an optional trailing comma.

use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_symbol_pool::{symbol, Symbol};

use crate::{
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    editions::{FeatureGate, UPGRADE_NOTE},
    parser::{ast::*, lexer::*},
    shared::*,
    MatchedFileCommentMap,
};

struct Context<'env, 'lexer, 'input> {
    package_name: Option<Symbol>,
    env: &'env mut CompilationEnv,
    tokens: &'lexer mut Lexer<'input>,
}

impl<'env, 'lexer, 'input> Context<'env, 'lexer, 'input> {
    fn new(
        env: &'env mut CompilationEnv,
        tokens: &'lexer mut Lexer<'input>,
        package_name: Option<Symbol>,
    ) -> Self {
        Self {
            package_name,
            env,
            tokens,
        }
    }
}

//**************************************************************************************************
// Error Handling
//**************************************************************************************************

fn current_token_error_string(tokens: &Lexer) -> String {
    if tokens.peek() == Tok::EOF {
        "end-of-file".to_string()
    } else {
        format!("'{}'", tokens.content())
    }
}

fn unexpected_token_error(tokens: &Lexer, expected: &str) -> Box<Diagnostic> {
    unexpected_token_error_(tokens, tokens.start_loc(), expected)
}

fn unexpected_token_error_(
    tokens: &Lexer,
    expected_start_loc: usize,
    expected: &str,
) -> Box<Diagnostic> {
    let unexpected_loc = current_token_loc(tokens);
    let unexpected = current_token_error_string(tokens);
    let expected_loc = if expected_start_loc < tokens.start_loc() {
        make_loc(
            tokens.file_hash(),
            expected_start_loc,
            tokens.previous_end_loc(),
        )
    } else {
        unexpected_loc
    };
    Box::new(diag!(
        Syntax::UnexpectedToken,
        (unexpected_loc, format!("Unexpected {}", unexpected)),
        (expected_loc, format!("Expected {}", expected)),
    ))
}

fn add_type_args_ambiguity_label(loc: Loc, mut diag: Box<Diagnostic>) -> Box<Diagnostic> {
    const MSG: &str = "Perhaps you need a blank space before this '<' operator?";
    diag.add_secondary_label((loc, MSG));
    diag
}

// A macro for providing better diagnostics when we expect a specific token and find some other
// pattern instead. For example, we can use this to handle the case when a const is missing its
// type annotation as:
//
//  expect_token!(
//      context.tokens,
//      Tok::Colon,
//      Tok::Equal => (Syntax::UnexpectedToken, name.loc(), "Add type annotation to this constant")
//  )?;
//
// This macro will fall through to an unexpected token error, but may also define its own default
// as well:
//
//  expect_token!(
//      context.tokens,
//      Tok::Colon,
//      Tok::Equal => (Syntax::UnexpectedToken, name.loc(), "Add type annotation to this constant")
//      _ => (Syntax::UnexpectedToken, name.loc(), "Misformed constant definition")
//  )?;
//
//  NB(cgswords): we could make $expected a pat if we required users to pass in a name for it as
//  well for the default-case error reporting.

macro_rules! expect_token {
    ($tokens:expr, $expected:expr, $($tok:pat => ($code:expr, $loc:expr, $msg:expr)),+) => {
        {
            let next = $tokens.peek();
            match next {
                _ if next == $expected => {
                    $tokens.advance()?;
                    Ok(())
                },
                $($tok => Err(Box::new(diag!($code, ($loc, $msg)))),)+
                _ => {
                    let expected = format!("'{}'{}", next, $expected);
                    Err(unexpected_token_error_($tokens, $tokens.start_loc(), &expected))
                },
            }
        }
    };
    ($tokens:expr,
     $expected:expr,
     $($tok:pat => ($code:expr, $loc:expr, $msg:expr)),+
     _ => ($dcode:expr, $dloc:expr, $dmsg:expr)) => {
        {
            let next = {
                $tokens.advance()?;
                Ok(())
            };
            match next {
                _ if next == $expected => Ok(()),
                $($tok => Err(Box::new(diag!($code, ($loc, $msg)))),)+
                _ => Err(Box::new(diag!($dcode, ($dloc, $dmsg)))),
            }
        }
    }
}

//**************************************************************************************************
// Miscellaneous Utilities
//**************************************************************************************************

pub fn make_loc(file_hash: FileHash, start: usize, end: usize) -> Loc {
    Loc::new(file_hash, start as u32, end as u32)
}

fn current_token_loc(tokens: &Lexer) -> Loc {
    let start_loc = tokens.start_loc();
    make_loc(
        tokens.file_hash(),
        start_loc,
        start_loc + tokens.content().len(),
    )
}

fn spanned<T>(file_hash: FileHash, start: usize, end: usize, value: T) -> Spanned<T> {
    Spanned {
        loc: make_loc(file_hash, start, end),
        value,
    }
}

// Check for the specified token and consume it if it matches.
// Returns true if the token matches.
fn match_token(tokens: &mut Lexer, tok: Tok) -> Result<bool, Box<Diagnostic>> {
    if tokens.peek() == tok {
        tokens.advance()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// Check for the specified token and return an error if it does not match.
fn consume_token(tokens: &mut Lexer, tok: Tok) -> Result<(), Box<Diagnostic>> {
    consume_token_(tokens, tok, tokens.start_loc(), "")
}

fn consume_token_(
    tokens: &mut Lexer,
    tok: Tok,
    expected_start_loc: usize,
    expected_case: &str,
) -> Result<(), Box<Diagnostic>> {
    if tokens.peek() == tok {
        tokens.advance()?;
        Ok(())
    } else {
        let expected = format!("'{}'{}", tok, expected_case);
        Err(unexpected_token_error_(
            tokens,
            expected_start_loc,
            &expected,
        ))
    }
}

// let unexp_loc = current_token_loc(tokens);
// let unexp_msg = format!("Unexpected {}", current_token_error_string(tokens));

// let end_loc = tokens.previous_end_loc();
// let addr_loc = make_loc(tokens.file_hash(), start_loc, end_loc);
// let exp_msg = format!("Expected '::' {}", case);
// Err(vec![(unexp_loc, unexp_msg), (addr_loc, exp_msg)])

// Check for the identifier token with specified value and return an error if it does not match.
fn consume_identifier(tokens: &mut Lexer, value: &str) -> Result<(), Box<Diagnostic>> {
    if tokens.peek() == Tok::Identifier && tokens.content() == value {
        tokens.advance()
    } else {
        let expected = format!("'{}'", value);
        Err(unexpected_token_error(tokens, &expected))
    }
}

// If the next token is the specified kind, consume it and return
// its source location.
fn consume_optional_token_with_loc(
    tokens: &mut Lexer,
    tok: Tok,
) -> Result<Option<Loc>, Box<Diagnostic>> {
    if tokens.peek() == tok {
        let start_loc = tokens.start_loc();
        tokens.advance()?;
        let end_loc = tokens.previous_end_loc();
        Ok(Some(make_loc(tokens.file_hash(), start_loc, end_loc)))
    } else {
        Ok(None)
    }
}

// While parsing a list and expecting a ">" token to mark the end, replace
// a ">>" token with the expected ">". This handles the situation where there
// are nested type parameters that result in two adjacent ">" tokens, e.g.,
// "A<B<C>>".
fn adjust_token(tokens: &mut Lexer, end_token: Tok) {
    if tokens.peek() == Tok::GreaterGreater && end_token == Tok::Greater {
        tokens.replace_token(Tok::Greater, 1);
    }
}

// Parse a comma-separated list of items, including the specified starting and
// ending tokens.
fn parse_comma_list<F, R>(
    context: &mut Context,
    start_token: Tok,
    end_token: Tok,
    parse_list_item: F,
    item_description: &str,
) -> Result<Vec<R>, Box<Diagnostic>>
where
    F: Fn(&mut Context) -> Result<R, Box<Diagnostic>>,
{
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, start_token)?;
    parse_comma_list_after_start(
        context,
        start_loc,
        start_token,
        end_token,
        parse_list_item,
        item_description,
    )
}

// Parse a comma-separated list of items, including the specified ending token, but
// assuming that the starting token has already been consumed.
fn parse_comma_list_after_start<F, R>(
    context: &mut Context,
    start_loc: usize,
    start_token: Tok,
    end_token: Tok,
    parse_list_item: F,
    item_description: &str,
) -> Result<Vec<R>, Box<Diagnostic>>
where
    F: Fn(&mut Context) -> Result<R, Box<Diagnostic>>,
{
    adjust_token(context.tokens, end_token);
    if match_token(context.tokens, end_token)? {
        return Ok(vec![]);
    }
    let mut v = vec![];
    loop {
        if context.tokens.peek() == Tok::Comma {
            let current_loc = context.tokens.start_loc();
            let loc = make_loc(context.tokens.file_hash(), current_loc, current_loc);
            return Err(Box::new(diag!(
                Syntax::UnexpectedToken,
                (loc, format!("Expected {}", item_description))
            )));
        }
        v.push(parse_list_item(context)?);
        adjust_token(context.tokens, end_token);
        if match_token(context.tokens, end_token)? {
            break Ok(v);
        }
        if !match_token(context.tokens, Tok::Comma)? {
            let current_loc = context.tokens.start_loc();
            let loc = make_loc(context.tokens.file_hash(), current_loc, current_loc);
            let loc2 = make_loc(context.tokens.file_hash(), start_loc, start_loc);
            return Err(Box::new(diag!(
                Syntax::UnexpectedToken,
                (loc, format!("Expected '{}'", end_token)),
                (loc2, format!("To match this '{}'", start_token)),
            )));
        }
        adjust_token(context.tokens, end_token);
        if match_token(context.tokens, end_token)? {
            break Ok(v);
        }
    }
}

// Parse a list of items, without specified start and end tokens, and the separator determined by
// the passed function `parse_list_continue`.
fn parse_list<C, F, R>(
    context: &mut Context,
    mut parse_list_continue: C,
    parse_list_item: F,
) -> Result<Vec<R>, Box<Diagnostic>>
where
    C: FnMut(&mut Context) -> Result<bool, Box<Diagnostic>>,
    F: Fn(&mut Context) -> Result<R, Box<Diagnostic>>,
{
    let mut v = vec![];
    loop {
        v.push(parse_list_item(context)?);
        if !parse_list_continue(context)? {
            break Ok(v);
        }
    }
}

// Helper for location blocks

macro_rules! with_loc {
    ($context:expr, $body:block) => {{
        let start_loc = $context.tokens.start_loc();
        let result = $body;
        let end_loc = $context.tokens.previous_end_loc();
        (
            make_loc($context.tokens.file_hash(), start_loc, end_loc),
            result,
        )
    }};
    ($context:expr, $body:expr) => {{
        let start_loc = $context.tokens.start_loc();
        let result = $body;
        let end_loc = $context.tokens.previous_end_loc();
        (
            make_loc($context.tokens.file_hash(), start_loc, end_loc),
            result,
        )
    }};
}

macro_rules! ok_with_loc {
    ($context:expr, $body:block) => {{
        let start_loc = $context.tokens.start_loc();
        let result = $body;
        let end_loc = $context.tokens.previous_end_loc();
        Ok(sp(
            make_loc($context.tokens.file_hash(), start_loc, end_loc),
            result,
        ))
    }};
    ($context:expr, $body:expr) => {{
        let start_loc = $context.tokens.start_loc();
        let result = $body;
        let end_loc = $context.tokens.previous_end_loc();
        Ok(sp(
            make_loc($context.tokens.file_hash(), start_loc, end_loc),
            result,
        ))
    }};
}

//**************************************************************************************************
// Identifiers, Addresses, and Names
//**************************************************************************************************

// Parse an identifier:
//      Identifier = <IdentifierValue>
#[allow(clippy::needless_if)]
fn parse_identifier(context: &mut Context) -> Result<Name, Box<Diagnostic>> {
    if matches!(
        context.tokens.peek(),
        Tok::Identifier | Tok::RestrictedIdentifier
    ) {}
    let id: Symbol = match context.tokens.peek() {
        Tok::Identifier => context.tokens.content().into(),
        Tok::RestrictedIdentifier => {
            // peel off backticks ``
            let content = context.tokens.content();
            let peeled = &content[1..content.len() - 1];
            peeled.into()
        }
        _ => {
            return Err(unexpected_token_error(context.tokens, "an identifier"));
        }
    };
    let start_loc = context.tokens.start_loc();
    context.tokens.advance()?;
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, id))
}

// Parse a numerical address value
//     NumericalAddress = <Number>
fn parse_address_bytes(
    context: &mut Context,
) -> Result<Spanned<NumericalAddress>, Box<Diagnostic>> {
    let loc = current_token_loc(context.tokens);
    let addr_res = NumericalAddress::parse_str(context.tokens.content());
    consume_token(context.tokens, Tok::NumValue)?;
    let addr_ = match addr_res {
        Ok(addr_) => addr_,
        Err(msg) => {
            context
                .env
                .add_diag(diag!(Syntax::InvalidAddress, (loc, msg)));
            NumericalAddress::DEFAULT_ERROR_ADDRESS
        }
    };
    Ok(sp(loc, addr_))
}

// Parse the beginning of an access, either an address or an identifier:
//      LeadingNameAccess = <NumericalAddress> | <Identifier>
fn parse_leading_name_access(context: &mut Context) -> Result<LeadingNameAccess, Box<Diagnostic>> {
    parse_leading_name_access_(context, false, || "an address or an identifier")
}

// Parse the beginning of an access, either an address or an identifier with a specific description
fn parse_leading_name_access_<'a, F: FnOnce() -> &'a str>(
    context: &mut Context,
    global_name: bool,
    item_description: F,
) -> Result<LeadingNameAccess, Box<Diagnostic>> {
    match context.tokens.peek() {
        Tok::RestrictedIdentifier | Tok::Identifier => {
            let loc = current_token_loc(context.tokens);
            let n = parse_identifier(context)?;
            let name = if global_name {
                LeadingNameAccess_::GlobalAddress(n)
            } else {
                LeadingNameAccess_::Name(n)
            };
            Ok(sp(loc, name))
        }
        Tok::NumValue => {
            let sp!(loc, addr) = parse_address_bytes(context)?;
            Ok(sp(loc, LeadingNameAccess_::AnonymousAddress(addr)))
        }
        _ => Err(unexpected_token_error(context.tokens, item_description())),
    }
}

// Parse a variable name:
//      Var = <Identifier>
fn parse_var(context: &mut Context) -> Result<Var, Box<Diagnostic>> {
    Ok(Var(parse_identifier(context)?))
}

// Parse a field name:
//      Field = <Identifier>
fn parse_field(context: &mut Context) -> Result<Field, Box<Diagnostic>> {
    Ok(Field(parse_identifier(context)?))
}

// Parse a module name:
//      ModuleName = <Identifier>
fn parse_module_name(context: &mut Context) -> Result<ModuleName, Box<Diagnostic>> {
    Ok(ModuleName(parse_identifier(context)?))
}

// Parse a module identifier:
//      ModuleIdent = <LeadingNameAccess> "::" <ModuleName>
//                  | "::" <LeadingNameAccess> "::" <ModuleName>

// Parse a module access (a variable, struct type, or function):
//      NameAccessChain =
//          <LeadingNameAccess> ( "::" <Identifier> ( "::" <Identifier> ( "::" <Identifier> )? )? )?
fn parse_name_access_chain<'a, F: FnOnce() -> &'a str>(
    context: &mut Context,
    item_description: F,
) -> Result<NameAccessChain, Box<Diagnostic>> {
    ok_with_loc!(
        context,
        if context.tokens.peek() == Tok::ColonColon {
            context.tokens.advance()?;
            parse_name_access_chain_(context, true, item_description)?
        } else {
            parse_name_access_chain_(context, false, item_description)?
        }
    )
}

// Parse a module access with a specific description
fn parse_name_access_chain_<'a, F: FnOnce() -> &'a str>(
    context: &mut Context,
    global_name: bool,
    item_description: F,
) -> Result<NameAccessChain_, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let ln = parse_leading_name_access_(context, global_name, item_description)?;
    let ln = match ln {
        // A name by itself is a valid access chain
        sp!(_, LeadingNameAccess_::Name(n1)) if context.tokens.peek() != Tok::ColonColon => {
            return Ok(NameAccessChain_::One(n1))
        }
        ln => ln,
    };

    if matches!(ln, sp!(_, LeadingNameAccess_::GlobalAddress(_)))
        && context.tokens.peek() != Tok::ColonColon
    {
        let mut diag = diag!(
            Syntax::UnexpectedToken,
            (
                ln.loc,
                "Expected '::' after the address in this module access chain"
            )
        );
        diag.add_note(
            "Access chains that start with '::' must be one of the following forms: \
            \n  '::<address>::<module>', '::<address>::<module>::<member>'",
        );
        return Err(Box::new(diag));
    }

    consume_token_(
        context.tokens,
        Tok::ColonColon,
        start_loc,
        " after an address in a module access chain",
    )?;
    let n2 = parse_identifier(context)?;
    if context.tokens.peek() != Tok::ColonColon {
        return Ok(NameAccessChain_::Two(ln, n2));
    }
    let ln_n2_loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    consume_token(context.tokens, Tok::ColonColon)?;
    let n3 = parse_identifier(context)?;
    if context.tokens.peek() != Tok::ColonColon {
        return Ok(NameAccessChain_::Three(sp(ln_n2_loc, (ln, n2)), n3));
    }
    consume_token(context.tokens, Tok::ColonColon)?;
    let n4 = parse_identifier(context)?;
    Ok(NameAccessChain_::Four(sp(ln_n2_loc, (ln, n2)), n3, n4))
}

//**************************************************************************************************
// Modifiers
//**************************************************************************************************

struct Modifiers {
    visibility: Option<Visibility>,
    entry: Option<Loc>,
    native: Option<Loc>,
}

impl Modifiers {
    fn empty() -> Self {
        Self {
            visibility: None,
            entry: None,
            native: None,
        }
    }
}

// Parse module member modifiers: visiblility and native.
//      ModuleMemberModifiers = <ModuleMemberModifier>*
//      ModuleMemberModifier = <Visibility> | "native"
// ModuleMemberModifiers checks for uniqueness, meaning each individual ModuleMemberModifier can
// appear only once
fn parse_module_member_modifiers(context: &mut Context) -> Result<Modifiers, Box<Diagnostic>> {
    let mut mods = Modifiers::empty();
    loop {
        match context.tokens.peek() {
            Tok::Public => {
                let vis = parse_visibility(context)?;
                if let Some(prev_vis) = mods.visibility {
                    let msg = "Duplicate visibility modifier".to_string();
                    let prev_msg = "Visibility modifier previously given here".to_string();
                    context.env.add_diag(diag!(
                        Declarations::DuplicateItem,
                        (vis.loc().unwrap(), msg),
                        (prev_vis.loc().unwrap(), prev_msg),
                    ));
                }
                mods.visibility = Some(vis)
            }
            Tok::Native => {
                let loc = current_token_loc(context.tokens);
                context.tokens.advance()?;
                if let Some(prev_loc) = mods.native {
                    let msg = "Duplicate 'native' modifier".to_string();
                    let prev_msg = "'native' modifier previously given here".to_string();
                    context.env.add_diag(diag!(
                        Declarations::DuplicateItem,
                        (loc, msg),
                        (prev_loc, prev_msg)
                    ))
                }
                mods.native = Some(loc)
            }
            Tok::Identifier if context.tokens.content() == ENTRY_MODIFIER => {
                let loc = current_token_loc(context.tokens);
                context.tokens.advance()?;
                if let Some(prev_loc) = mods.entry {
                    let msg = format!("Duplicate '{}' modifier", ENTRY_MODIFIER);
                    let prev_msg = format!("'{}' modifier previously given here", ENTRY_MODIFIER);
                    context.env.add_diag(diag!(
                        Declarations::DuplicateItem,
                        (loc, msg),
                        (prev_loc, prev_msg)
                    ))
                }
                mods.entry = Some(loc)
            }
            _ => break,
        }
    }
    Ok(mods)
}

// Parse a function visibility modifier:
//      Visibility = "public" ( "( "friend" | "package" ")" )?
fn parse_visibility(context: &mut Context) -> Result<Visibility, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, Tok::Public)?;
    let sub_public_vis = if match_token(context.tokens, Tok::LParen)? {
        let (sub_token, tok_content) = (context.tokens.peek(), context.tokens.content());
        context.tokens.advance()?;
        if sub_token != Tok::RParen {
            consume_token(context.tokens, Tok::RParen)?;
        }
        Some((sub_token, tok_content))
    } else {
        None
    };
    let end_loc = context.tokens.previous_end_loc();
    // this loc will cover the span of 'public' or 'public(...)' in entirety
    let loc = make_loc(context.tokens.file_hash(), start_loc, end_loc);
    Ok(match sub_public_vis {
        None => Visibility::Public(loc),
        Some((Tok::Friend, _)) => Visibility::Friend(loc),
        Some((Tok::Identifier, Visibility::PACKAGE_IDENT)) => Visibility::Package(loc),
        _ => {
            let msg = format!(
                "Invalid visibility modifier. Consider removing it or using '{}', '{}' or '{}'",
                Visibility::PUBLIC,
                Visibility::FRIEND,
                Visibility::PACKAGE,
            );
            return Err(Box::new(diag!(Syntax::UnexpectedToken, (loc, msg))));
        }
    })
}
// Parse an attribute value. Either a value literal or a module access
//      AttributeValue =
//          <Value>
//          | <NameAccessChain>
fn parse_attribute_value(context: &mut Context) -> Result<AttributeValue, Box<Diagnostic>> {
    if let Some(v) = maybe_parse_value(context)? {
        return Ok(sp(v.loc, AttributeValue_::Value(v)));
    }

    let ma = parse_name_access_chain(context, || "attribute name value")?;
    Ok(sp(ma.loc, AttributeValue_::ModuleAccess(ma)))
}

// Parse a single attribute
//      Attribute =
//          <Identifier>
//          | <Identifier> "=" <AttributeValue>
//          | <Identifier> "(" Comma<Attribute> ")"
fn parse_attribute(context: &mut Context) -> Result<Attribute, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let n = parse_identifier(context)?;
    let attr_ = match context.tokens.peek() {
        Tok::Equal => {
            context.tokens.advance()?;
            Attribute_::Assigned(n, Box::new(parse_attribute_value(context)?))
        }
        Tok::LParen => {
            let args_ = parse_comma_list(
                context,
                Tok::LParen,
                Tok::RParen,
                parse_attribute,
                "attribute",
            )?;
            let end_loc = context.tokens.previous_end_loc();
            Attribute_::Parameterized(
                n,
                spanned(context.tokens.file_hash(), start_loc, end_loc, args_),
            )
        }
        _ => Attribute_::Name(n),
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        attr_,
    ))
}

// Parse attributes. Used to annotate a variety of AST nodes
//      Attributes = ("#" "[" Comma<Attribute> "]")*
fn parse_attributes(context: &mut Context) -> Result<Vec<Attributes>, Box<Diagnostic>> {
    let mut attributes_vec = vec![];
    while let Tok::NumSign = context.tokens.peek() {
        let start_loc = context.tokens.start_loc();
        context.tokens.advance()?;
        let attributes_ = parse_comma_list(
            context,
            Tok::LBracket,
            Tok::RBracket,
            parse_attribute,
            "attribute",
        )?;
        let end_loc = context.tokens.previous_end_loc();
        attributes_vec.push(spanned(
            context.tokens.file_hash(),
            start_loc,
            end_loc,
            attributes_,
        ))
    }
    Ok(attributes_vec)
}

//**************************************************************************************************
// Fields and Bindings
//**************************************************************************************************

// Parse a field name optionally followed by a colon and an expression argument:
//      ExpField = <Field> <":" <Exp>>?
fn parse_exp_field(context: &mut Context) -> Result<(Field, Exp), Box<Diagnostic>> {
    let f = parse_field(context)?;
    let arg = if match_token(context.tokens, Tok::Colon)? {
        parse_exp(context)?
    } else {
        sp(
            f.loc(),
            Exp_::Name(sp(f.loc(), NameAccessChain_::One(f.0)), None),
        )
    };
    Ok((f, arg))
}

// Parse a field name optionally followed by a colon and a binding:
//      BindField =
//          <Field> <":" <Bind>>?
//          | "mut" <Field>
//
// If the binding is not specified, the default is to use a variable
// with the same name as the field.
fn parse_bind_field(context: &mut Context) -> Result<(Field, Bind), Box<Diagnostic>> {
    if context.tokens.peek() == Tok::Mut {
        let start_loc = context.tokens.start_loc();
        context.tokens.advance()?;
        let end_loc = context.tokens.previous_end_loc();
        let mut_loc = make_loc(context.tokens.file_hash(), start_loc, end_loc);
        let f = parse_field(context)?;
        let arg = sp(f.loc(), Bind_::Var(Some(mut_loc), Var(f.0)));
        Ok((f, arg))
    } else {
        let f = parse_field(context)?;
        let arg = if match_token(context.tokens, Tok::Colon)? {
            parse_bind(context)?
        } else {
            sp(f.loc(), Bind_::Var(None, Var(f.0)))
        };
        Ok((f, arg))
    }
}

// Parse a binding:
//      Bind =
//          "mut"? <Var>
//          | <NameAccessChain> <OptionalTypeArgs> "{" Comma<BindField> "}"
//          | <NameAccessChain> <OptionalTypeArgs> "(" Comma<Bind> ")"
fn parse_bind(context: &mut Context) -> Result<Bind, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    if matches!(
        context.tokens.peek(),
        Tok::Identifier | Tok::RestrictedIdentifier | Tok::Mut
    ) {
        let next_tok = context.tokens.lookahead()?;
        if !matches!(
            next_tok,
            Tok::LBrace | Tok::Less | Tok::ColonColon | Tok::LParen
        ) {
            let mut_ = if context.tokens.peek() == Tok::Mut {
                context.tokens.advance()?;
                let end_loc = context.tokens.previous_end_loc();
                Some(make_loc(context.tokens.file_hash(), start_loc, end_loc))
            } else {
                None
            };
            let v = Bind_::Var(mut_, parse_var(context)?);
            let end_loc = context.tokens.previous_end_loc();
            return Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, v));
        }
    }
    // The item description specified here should include the special case above for
    // variable names, because if the current context cannot be parsed as a struct name
    // it is possible that the user intention was to use a variable name.
    let ty = parse_name_access_chain(context, || "a variable or struct name")?;
    let ty_args = parse_optional_type_args(context)?;
    let args = if context.tokens.peek() == Tok::LParen {
        let current_loc = current_token_loc(context.tokens);
        context.env.check_feature(
            FeatureGate::PositionalFields,
            context.package_name,
            current_loc,
        );
        let args = parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            parse_bind,
            "a field binding",
        )?;
        FieldBindings::Positional(args)
    } else {
        let args = parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            parse_bind_field,
            "a field binding",
        )?;
        FieldBindings::Named(args)
    };
    let end_loc = context.tokens.previous_end_loc();
    let unpack = Bind_::Unpack(Box::new(ty), ty_args, args);
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        unpack,
    ))
}

// Parse a list of bindings, which can be zero, one, or more bindings:
//      BindList =
//          <Bind>
//          | "(" Comma<Bind> ")"
//
// The list is enclosed in parenthesis, except that the parenthesis are
// optional if there is a single Bind.
fn parse_bind_list(context: &mut Context) -> Result<BindList, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let b = if context.tokens.peek() != Tok::LParen {
        vec![parse_bind(context)?]
    } else {
        parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            parse_bind,
            "a variable or structure binding",
        )?
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, b))
}

// Parse a list of bindings for lambda.
//      LambdaBindList =
//          "|" Comma<Bind> "|"
fn parse_lambda_bind_list(context: &mut Context) -> Result<BindList, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let b = parse_comma_list(
        context,
        Tok::Pipe,
        Tok::Pipe,
        parse_bind,
        "a variable or structure binding",
    )?;
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, b))
}

//**************************************************************************************************
// Values
//**************************************************************************************************

// Parse a byte string:
//      ByteString = <ByteStringValue>
fn parse_byte_string(context: &mut Context) -> Result<Value_, Box<Diagnostic>> {
    if context.tokens.peek() != Tok::ByteStringValue {
        return Err(unexpected_token_error(
            context.tokens,
            "a byte string value",
        ));
    }
    let s = context.tokens.content();
    let text = Symbol::from(&s[2..s.len() - 1]);
    let value_ = if s.starts_with("x\"") {
        Value_::HexString(text)
    } else {
        assert!(s.starts_with("b\""));
        Value_::ByteString(text)
    };
    context.tokens.advance()?;
    Ok(value_)
}

// Parse a value:
//      Value =
//          "@" <LeadingAccessName>
//          | "true"
//          | "false"
//          | <Number>
//          | <NumberTyped>
//          | <ByteString>
fn maybe_parse_value(context: &mut Context) -> Result<Option<Value>, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let val = match context.tokens.peek() {
        Tok::AtSign => {
            context.tokens.advance()?;
            let addr = parse_leading_name_access(context)?;
            Value_::Address(addr)
        }
        Tok::True => {
            context.tokens.advance()?;
            Value_::Bool(true)
        }
        Tok::False => {
            context.tokens.advance()?;
            Value_::Bool(false)
        }
        Tok::NumValue => {
            //  If the number is followed by "::", parse it as the beginning of an address access
            if let Ok(Tok::ColonColon) = context.tokens.lookahead() {
                return Ok(None);
            }
            let num = context.tokens.content().into();
            context.tokens.advance()?;
            Value_::Num(num)
        }
        Tok::NumTypedValue => {
            let num = context.tokens.content().into();
            context.tokens.advance()?;
            Value_::Num(num)
        }

        Tok::ByteStringValue => parse_byte_string(context)?,
        _ => return Ok(None),
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(Some(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        val,
    )))
}

fn parse_value(context: &mut Context) -> Result<Value, Box<Diagnostic>> {
    Ok(maybe_parse_value(context)?.expect("parse_value called with invalid token"))
}

//**************************************************************************************************
// Sequences
//**************************************************************************************************

// Parse a sequence item:
//      SequenceItem =
//          <Exp>
//          | "let" <BindList> (":" <Type>)? ("=" <Exp>)?
fn parse_sequence_item(context: &mut Context) -> Result<SequenceItem, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let item = if match_token(context.tokens, Tok::Let)? {
        let b = parse_bind_list(context)?;
        let ty_opt = if match_token(context.tokens, Tok::Colon)? {
            Some(parse_type(context)?)
        } else {
            None
        };
        if match_token(context.tokens, Tok::Equal)? {
            let e = parse_exp(context)?;
            SequenceItem_::Bind(b, ty_opt, Box::new(e))
        } else {
            SequenceItem_::Declare(b, ty_opt)
        }
    } else {
        let e = parse_exp(context)?;
        SequenceItem_::Seq(Box::new(e))
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        item,
    ))
}

// Parse a sequence:
//      Sequence = <UseDecl>* (<SequenceItem> ";")* <Exp>? "}"
//
// Note that this does not include the opening brace of a block but it
// does consume the closing right brace.
fn parse_sequence(context: &mut Context) -> Result<Sequence, Box<Diagnostic>> {
    let mut uses = vec![];
    while context.tokens.peek() == Tok::Use {
        let start_loc = context.tokens.start_loc();
        uses.push(parse_use_decl(
            vec![],
            start_loc,
            Modifiers::empty(),
            context,
        )?);
    }

    let mut seq: Vec<SequenceItem> = vec![];
    let mut last_semicolon_loc = None;
    let mut eopt = None;
    while context.tokens.peek() != Tok::RBrace {
        let item = parse_sequence_item(context)?;
        if context.tokens.peek() == Tok::RBrace {
            // If the sequence ends with an expression that is not
            // followed by a semicolon, split out that expression
            // from the rest of the SequenceItems.
            match item.value {
                SequenceItem_::Seq(e) => {
                    eopt = Some(Spanned {
                        loc: item.loc,
                        value: e.value,
                    });
                }
                _ => return Err(unexpected_token_error(context.tokens, "';'")),
            }
            break;
        }
        seq.push(item);
        last_semicolon_loc = Some(current_token_loc(context.tokens));
        consume_token(context.tokens, Tok::Semicolon)?;
    }
    context.tokens.advance()?; // consume the RBrace
    Ok((uses, seq, last_semicolon_loc, Box::new(eopt)))
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

// Parse an expression term:
//      Term =
//          "break" <Exp>?
//          | "continue"
//          | "vector" ('<' Comma<Type> ">")? "[" Comma<Exp> "]"
//          | <Value>
//          | "(" Comma<Exp> ")"
//          | "(" <Exp> ":" <Type> ")"
//          | "(" <Exp> "as" <Type> ")"
//          | <Label> <Exp>
//          | "{" <Sequence>
//          | "if" "(" <Exp> ")" <Exp> "else" "{" <Exp> "}"
//          | "if" "(" <Exp> ")" "{" <Exp> "}"
//          | "if" "(" <Exp> ")" <Exp> ("else" <Exp>)?
//          | "while" "(" <Exp> ")" "{" <Exp> "}"
//          | "while" "(" <Exp> ")" <Exp> (SpecBlock)?
//          | "loop" <Exp>
//          | "loop" "{" <Exp> "}"
//          | "return" "{" <Exp> "}"
//          | "return" <Exp>?
//          | "abort" "{" <Exp> "}"
//          | "abort" <Exp>
//          | "match" <Exp> "{" (<MatchArm> ",")+ "}"
fn parse_term(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    const VECTOR_IDENT: &str = "vector";

    let start_loc = context.tokens.start_loc();
    let term = match context.tokens.peek() {
        tok if is_control_exp(tok) => {
            let (control_exp, ends_in_block) = parse_control_exp(context)?;
            if !ends_in_block || at_end_of_exp(context) {
                return Ok(control_exp);
            }

            return parse_binop_exp(context, control_exp, /* min_prec */ 1);
        }

        Tok::Identifier
            if context.tokens.content() == VECTOR_IDENT
                && matches!(context.tokens.lookahead(), Ok(Tok::Less | Tok::LBracket)) =>
        {
            consume_identifier(context.tokens, VECTOR_IDENT)?;
            let vec_end_loc = context.tokens.previous_end_loc();
            let vec_loc = make_loc(context.tokens.file_hash(), start_loc, vec_end_loc);
            let targs_start_loc = context.tokens.start_loc();
            let tys_opt = parse_optional_type_args(context).map_err(|diag| {
                let targ_loc =
                    make_loc(context.tokens.file_hash(), targs_start_loc, targs_start_loc);
                add_type_args_ambiguity_label(targ_loc, diag)
            })?;
            let args_start_loc = context.tokens.start_loc();
            let args_ = parse_comma_list(
                context,
                Tok::LBracket,
                Tok::RBracket,
                parse_exp,
                "a vector argument expression",
            )?;
            let args_end_loc = context.tokens.previous_end_loc();
            let args = spanned(
                context.tokens.file_hash(),
                args_start_loc,
                args_end_loc,
                args_,
            );
            Exp_::Vector(vec_loc, tys_opt, args)
        }

        Tok::ColonColon
            if context
                .env
                .supports_feature(context.package_name, FeatureGate::Move2024Paths) =>
        {
            if context.tokens.lookahead()? == Tok::Identifier {
                parse_name_exp(context)?
            } else {
                return Err(unexpected_token_error(
                    context.tokens,
                    "An identifier after '::'",
                ));
            }
        }
        Tok::Identifier | Tok::RestrictedIdentifier => parse_name_exp(context)?,

        Tok::BlockLabel => {
            // TODO: improve error messages around this.
            let label = parse_block_label(context)?;
            consume_token(context.tokens, Tok::Colon)?;
            consume_token(context.tokens, Tok::LBrace)?;
            Exp_::NamedBlock(label, parse_sequence(context)?)
        }

        Tok::NumValue => {
            // Check if this is a ModuleIdent (in a ModuleAccess).
            if context.tokens.lookahead()? == Tok::ColonColon {
                parse_name_exp(context)?
            } else {
                Exp_::Value(parse_value(context)?)
            }
        }

        Tok::AtSign | Tok::True | Tok::False | Tok::NumTypedValue | Tok::ByteStringValue => {
            Exp_::Value(parse_value(context)?)
        }

        // "(" Comma<Exp> ")"
        // "(" <Exp> ":" <Type> ")"
        // "(" <Exp> "as" <Type> ")"
        Tok::LParen => {
            let list_loc = context.tokens.start_loc();
            context.tokens.advance()?; // consume the LParen
            if match_token(context.tokens, Tok::RParen)? {
                Exp_::Unit
            } else {
                // If there is a single expression inside the parens,
                // then it may be followed by a colon and a type annotation.
                let e = parse_exp(context)?;
                if match_token(context.tokens, Tok::Colon)? {
                    let ty = parse_type(context)?;
                    consume_token(context.tokens, Tok::RParen)?;
                    Exp_::Annotate(Box::new(e), ty)
                } else if match_token(context.tokens, Tok::As)? {
                    let ty = parse_type(context)?;
                    consume_token(context.tokens, Tok::RParen)?;
                    Exp_::Cast(Box::new(e), ty)
                } else {
                    if context.tokens.peek() != Tok::RParen {
                        consume_token(context.tokens, Tok::Comma)?;
                    }
                    let mut es = parse_comma_list_after_start(
                        context,
                        list_loc,
                        Tok::LParen,
                        Tok::RParen,
                        parse_exp,
                        "an expression",
                    )?;
                    if es.is_empty() {
                        e.value
                    } else {
                        es.insert(0, e);
                        Exp_::ExpList(es)
                    }
                }
            }
        }

        // "{" <Sequence>
        Tok::LBrace => {
            context.tokens.advance()?; // consume the LBrace
            Exp_::Block(parse_sequence(context)?)
        }

        Tok::Spec => {
            let spec_block = parse_spec_block(vec![], context)?;
            Exp_::Spec(spec_block)
        }

        _ => {
            return Err(unexpected_token_error(context.tokens, "an expression term"));
        }
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        term,
    ))
}

fn is_control_exp(tok: Tok) -> bool {
    matches!(
        tok,
        Tok::Break
            | Tok::Continue
            | Tok::If
            | Tok::While
            | Tok::Loop
            | Tok::Return
            | Tok::Abort
            | Tok::Match
    )
}

fn parse_block_label(context: &mut Context) -> Result<BlockLabel, Box<Diagnostic>> {
    let id: Symbol = match context.tokens.peek() {
        Tok::BlockLabel => {
            // peel off leading tick '
            let content = context.tokens.content();
            let peeled = &content[1..content.len()];
            peeled.into()
        }
        _ => {
            return Err(unexpected_token_error(context.tokens, "a block identifier"));
        }
    };
    let start_loc = context.tokens.start_loc();
    context.tokens.advance()?;
    let end_loc = context.tokens.previous_end_loc();
    Ok(BlockLabel(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        id,
    )))
}

// if there is a block, only parse the block, not any subsequent tokens
// e.g.           if (cond) e1 else { e2 } + 1
// should be,    (if (cond) e1 else { e2 }) + 1
// AND NOT,       if (cond) e1 else ({ e2 } + 1)
// But otherwise, if (cond) e1 else e2 + 1
// should be,     if (cond) e1 else (e2 + 1)
fn parse_control_exp(context: &mut Context) -> Result<(Exp, bool), Box<Diagnostic>> {
    fn parse_exp_or_sequence(context: &mut Context) -> Result<(Exp, bool), Box<Diagnostic>> {
        match context.tokens.peek() {
            Tok::LBrace => {
                let block_start_loc = context.tokens.start_loc();
                context.tokens.advance()?; // consume the LBrace
                let block_ = Exp_::Block(parse_sequence(context)?);
                let block_end_loc = context.tokens.previous_end_loc();
                let exp = spanned(
                    context.tokens.file_hash(),
                    block_start_loc,
                    block_end_loc,
                    block_,
                );
                Ok((exp, true))
            }
            _ => Ok((parse_exp(context)?, false)),
        }
    }
    let start_loc = context.tokens.start_loc();
    let (exp_, ends_in_block) = match context.tokens.peek() {
        Tok::If => {
            context.tokens.advance()?;
            consume_token(context.tokens, Tok::LParen)?;
            let eb = Box::new(parse_exp(context)?);
            consume_token(context.tokens, Tok::RParen)?;
            let (et, ends_in_block) = parse_exp_or_sequence(context)?;
            let (ef, ends_in_block) = if match_token(context.tokens, Tok::Else)? {
                let (ef, ends_in_block) = parse_exp_or_sequence(context)?;
                (Some(Box::new(ef)), ends_in_block)
            } else {
                (None, ends_in_block)
            };
            (Exp_::IfElse(eb, Box::new(et), ef), ends_in_block)
        }
        Tok::While => {
            context.tokens.advance()?;
            consume_token(context.tokens, Tok::LParen)?;
            let econd = parse_exp(context)?;
            consume_token(context.tokens, Tok::RParen)?;
            let (eloop, ends_in_block) = parse_exp_or_sequence(context)?;
            let (econd, ends_in_block) = if context.tokens.peek() == Tok::Spec {
                // Parse a loop invariant. Also validate that only `invariant`
                // properties are contained in the spec block. This is
                // transformed into `while ({spec { .. }; cond) body`.
                let spec = parse_spec_block(vec![], context)?;
                for member in &spec.value.members {
                    match member.value {
                        // Ok
                        SpecBlockMember_::Condition {
                            kind: sp!(_, SpecConditionKind_::Invariant(..)),
                            ..
                        } => (),
                        _ => {
                            return Err(Box::new(diag!(
                                Syntax::InvalidSpecBlockMember,
                                (member.loc, "only 'invariant' allowed here")
                            )))
                        }
                    }
                }
                let spec_seq = sp(
                    spec.loc,
                    SequenceItem_::Seq(Box::new(sp(spec.loc, Exp_::Spec(spec)))),
                );
                let loc = econd.loc;
                let spec_block = Exp_::Block((vec![], vec![spec_seq], None, Box::new(Some(econd))));
                (sp(loc, spec_block), true)
            } else {
                (econd, ends_in_block)
            };
            (Exp_::While(Box::new(econd), Box::new(eloop)), ends_in_block)
        }
        Tok::Loop => {
            context.tokens.advance()?;
            let (eloop, ends_in_block) = parse_exp_or_sequence(context)?;
            (Exp_::Loop(Box::new(eloop)), ends_in_block)
        }
        Tok::Return => {
            context.tokens.advance()?;
            let label = match context.tokens.peek() {
                Tok::BlockLabel => Some(parse_block_label(context)?),
                _ => None,
            };
            let (e, ends_in_block) = if !at_start_of_exp(context) {
                (None, false)
            } else {
                let (e, ends_in_block) = parse_exp_or_sequence(context)?;
                (Some(Box::new(e)), ends_in_block)
            };
            (Exp_::Return(label, e), ends_in_block)
        }
        Tok::Abort => {
            context.tokens.advance()?;
            let (e, ends_in_block) = parse_exp_or_sequence(context)?;
            (Exp_::Abort(Box::new(e)), ends_in_block)
        }
        Tok::Break => {
            context.tokens.advance()?;
            let label = match context.tokens.peek() {
                Tok::BlockLabel => Some(parse_block_label(context)?),
                _ => None,
            };
            let (e, ends_in_block) = if !at_start_of_exp(context) {
                (None, false)
            } else {
                let (e, ends_in_block) = parse_exp_or_sequence(context)?;
                (Some(Box::new(e)), ends_in_block)
            };
            (Exp_::Break(label, e), ends_in_block)
        }
        Tok::Continue => {
            context.tokens.advance()?;
            let label = match context.tokens.peek() {
                Tok::BlockLabel => Some(parse_block_label(context)?),
                _ => None,
            };
            (Exp_::Continue(label), false)
        }
        Tok::Match => {
            context.tokens.advance()?;
            consume_token(context.tokens, Tok::LParen)?;
            let subject_exp = Box::new(parse_exp(context)?);
            consume_token(context.tokens, Tok::RParen)?;
            let arms = parse_match_arms(context)?;
            let result = Exp_::Match(subject_exp, arms);
            (result, true)
        }
        _ => unreachable!(),
    };
    let end_loc = context.tokens.previous_end_loc();
    let exp = spanned(context.tokens.file_hash(), start_loc, end_loc, exp_);
    Ok((exp, ends_in_block))
}

// Parse a pack, call, or other reference to a name:
//      NameExp =
//          <NameAccessChain> <OptionalTypeArgs> "{" Comma<ExpField> "}"
//          | <NameAccessChain> <OptionalTypeArgs> "(" Comma<Exp> ")"
//          | <NameAccessChain> "!" "(" Comma<Exp> ")"
//          | <NameAccessChain> <OptionalTypeArgs>
fn parse_name_exp(context: &mut Context) -> Result<Exp_, Box<Diagnostic>> {
    let name = parse_name_access_chain(context, || {
        panic!("parse_name_exp with something other than a ModuleAccess")
    })?;

    // There's an ambiguity if the name is followed by a '<'. If there is no whitespace
    // after the name, treat it as the start of a list of type arguments. Otherwise
    // assume that the '<' is a boolean operator.
    let mut tys = None;
    if context.tokens.peek() == Tok::Exclaim {
        // TODO(macro) handle type arguments
        context.tokens.advance()?;
        let is_macro = true;
        let rhs = parse_call_args(context)?;
        return Ok(Exp_::Call(name, is_macro, tys, rhs));
    }
    let start_loc = context.tokens.start_loc();

    if context.tokens.peek() == Tok::Less && name.loc.end() as usize == start_loc {
        let loc = make_loc(context.tokens.file_hash(), start_loc, start_loc);
        tys = parse_optional_type_args(context)
            .map_err(|diag| add_type_args_ambiguity_label(loc, diag))?;
    }

    match context.tokens.peek() {
        // Pack: "{" Comma<ExpField> "}"
        Tok::LBrace => {
            let fs = parse_comma_list(
                context,
                Tok::LBrace,
                Tok::RBrace,
                parse_exp_field,
                "a field expression",
            )?;
            Ok(Exp_::Pack(name, tys, fs))
        }

        // Call: "(" Comma<Exp> ")"
        Tok::Exclaim | Tok::LParen => {
            let is_macro = false;
            let rhs = parse_call_args(context)?;
            Ok(Exp_::Call(name, is_macro, tys, rhs))
        }

        // Other name reference...
        _ => Ok(Exp_::Name(name, tys)),
    }
}

// Parse the arguments to a call: "(" Comma<Exp> ")"
fn parse_call_args(context: &mut Context) -> Result<Spanned<Vec<Exp>>, Box<Diagnostic>> {
    ok_with_loc!(
        context,
        parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            parse_exp,
            "a call argument expression",
        )?
    )
}

// Parses a series of match arms, such as for a match block body "{" (<MatchArm>,)+ "}"
fn parse_match_arms(context: &mut Context) -> Result<Spanned<Vec<MatchArm>>, Box<Diagnostic>> {
    // FIXME check the arms are non-empty
    ok_with_loc!(
        context,
        parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            parse_match_arm,
            "a call argument expression",
        )?
    )
}

// Parses a match arm:
//   <MatchArm> = <MatchPat> ("if" <Exp>)? "=>" ("{" <Exp> "}" | <Exp>)
//
fn parse_match_arm(context: &mut Context) -> Result<MatchArm, Box<Diagnostic>> {
    ok_with_loc!(context, {
        let pattern = parse_match_pattern(context)?;
        let guard = match context.tokens.peek() {
            Tok::If => {
                context.tokens.advance()?;
                consume_token(context.tokens, Tok::LParen)?;
                let guard_exp = parse_exp(context)?;
                consume_token(context.tokens, Tok::RParen)?;
                Some(Box::new(guard_exp))
            }
            _ => None,
        };
        consume_token(context.tokens, Tok::EqualGreater)?;
        let rhs = match context.tokens.peek() {
            Tok::LBrace => {
                let block_start_loc = context.tokens.start_loc();
                context.tokens.advance()?; // consume the LBrace
                let block_ = Exp_::Block(parse_sequence(context)?);
                let block_end_loc = context.tokens.previous_end_loc();
                let exp = spanned(
                    context.tokens.file_hash(),
                    block_start_loc,
                    block_end_loc,
                    block_,
                );
                Box::new(exp)
            }
            _ => Box::new(parse_exp(context)?),
        };
        MatchArm_ {
            pattern,
            guard,
            rhs,
        }
    })
}

fn parse_match_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
    const WILDCARD_AT_ERROR_MSG: &str = "Can't use '_' as a binder in an '@' pattern";
    const INVALID_PAT_ERROR_MSG: &str = "Invalid pattern";

    use MatchPattern_::*;

    fn parse_ctor_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
        match context.tokens.peek() {
            Tok::LParen => {
                context.tokens.advance()?;
                let pat = parse_match_pattern(context);
                consume_token(context.tokens, Tok::RParen)?;
                pat
            }
            Tok::Identifier if context.tokens.content() == "_" => {
                context.tokens.advance()?;
                Ok(sp(
                    context.tokens.current_token_loc(),
                    MatchPattern_::Wildcard,
                ))
            }
            Tok::Identifier => ok_with_loc!(context, {
                let name_access_chain = parse_name_access_chain(context, || "a pattern entry")?;
                let ty_args = parse_optional_type_args(context)?;

                match context.tokens.peek() {
                    Tok::LParen => {
                        let (loc, patterns) = with_loc!(
                            context,
                            parse_comma_list(
                                context,
                                Tok::LParen,
                                Tok::RParen,
                                parse_match_pattern,
                                "a pattern",
                            )?
                        );
                        PositionalConstructor(name_access_chain, ty_args, sp(loc, patterns))
                    }
                    Tok::LBrace => {
                        let (loc, patterns) = with_loc!(
                            context,
                            parse_comma_list(
                                context,
                                Tok::LBrace,
                                Tok::RBrace,
                                parse_field_pattern,
                                "a field pattern",
                            )?
                        );
                        FieldConstructor(name_access_chain, ty_args, sp(loc, patterns))
                    }
                    _ => Name(name_access_chain, ty_args),
                }
            }),
            _ => {
                if let Some(value) = maybe_parse_value(context)? {
                    Ok(sp(value.loc, Literal(value)))
                } else {
                    Err(Box::new(diag!(
                        Syntax::UnexpectedToken,
                        (context.tokens.current_token_loc(), INVALID_PAT_ERROR_MSG)
                    )))
                }
            }
        }
    }

    fn parse_field_pattern(
        context: &mut Context,
    ) -> Result<(Field, MatchPattern), Box<Diagnostic>> {
        let field = parse_field(context)?;
        let pattern = if match_token(context.tokens, Tok::Colon)? {
            parse_match_pattern(context)?
        } else {
            sp(
                field.loc(),
                Name(sp(field.loc(), NameAccessChain_::One(field.0)), None),
            )
        };
        Ok((field, pattern))
    }

    fn parse_at_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
        match context.tokens.peek() {
            Tok::Identifier if context.tokens.lookahead() == Ok(Tok::AtSign) => {
                if context.tokens.content() == "_" {
                    Err(Box::new(diag!(
                        Syntax::UnexpectedToken,
                        (context.tokens.current_token_loc(), WILDCARD_AT_ERROR_MSG)
                    )))
                } else {
                    ok_with_loc!(context, {
                        let binder = parse_var(context)?;
                        consume_token(context.tokens, Tok::AtSign)?;
                        let rhs = parse_ctor_pattern(context)?;
                        MatchPattern_::At(binder, Box::new(rhs))
                    })
                }
            }
            _ => parse_ctor_pattern(context),
        }
    }

    ok_with_loc!(context, {
        let lhs = parse_at_pattern(context)?;
        if matches!(context.tokens.peek(), Tok::Pipe) {
            context.tokens.advance()?;
            let rhs = parse_at_pattern(context)?;
            MatchPattern_::Or(Box::new(lhs), Box::new(rhs))
        } else {
            lhs.value
        }
    })
}

// Return true if the current token is one that might occur after an Exp.
// This is needed, for example, to check for the optional Exp argument to
// a return (where "return" is itself an Exp).
fn at_end_of_exp(context: &mut Context) -> bool {
    matches!(
        context.tokens.peek(),
        // These are the tokens that can occur after an Exp. If the grammar
        // changes, we need to make sure that these are kept up to date and that
        // none of these tokens can occur at the beginning of an Exp.
        Tok::Else | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Colon | Tok::Semicolon
    )
}

fn at_start_of_exp(context: &mut Context) -> bool {
    matches!(
        context.tokens.peek(),
        // value
        Tok::NumValue
            | Tok::NumTypedValue
            | Tok::ByteStringValue
            | Tok::Identifier
            | Tok::RestrictedIdentifier
            | Tok::AtSign
            | Tok::Copy
            | Tok::Move
            | Tok::False
            | Tok::True
            | Tok::Amp
            | Tok::AmpMut
            | Tok::Star
            | Tok::Exclaim
            | Tok::LParen
            | Tok::LBrace
            | Tok::Abort
            | Tok::Break
            | Tok::Continue
            | Tok::If
            | Tok::Loop
            | Tok::Return
            | Tok::While
    )
}

// Parse an expression:
//      Exp =
//            <LambdaBindList> <Exp>        spec only
//          | <Quantifier>                  spec only
//          | <BinOpExp>
//          | <UnaryExp> "=" <Exp>
fn parse_exp(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let exp = match context.tokens.peek() {
        Tok::Pipe => {
            let bindings = parse_lambda_bind_list(context)?;
            let body = Box::new(parse_exp(context)?);
            Exp_::Lambda(bindings, body)
        }
        Tok::Identifier if is_quant(context) => parse_quant(context)?,
        _ => {
            // This could be either an assignment or a binary operator
            // expression.
            let lhs = parse_unary_exp(context)?;
            if context.tokens.peek() != Tok::Equal {
                return parse_binop_exp(context, lhs, /* min_prec */ 1);
            }
            context.tokens.advance()?; // consume the "="
            let rhs = Box::new(parse_exp(context)?);
            Exp_::Assign(Box::new(lhs), rhs)
        }
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, exp))
}

// Get the precedence of a binary operator. The minimum precedence value
// is 1, and larger values have higher precedence. For tokens that are not
// binary operators, this returns a value of zero so that they will be
// below the minimum value and will mark the end of the binary expression
// for the code in parse_binop_exp.
fn get_precedence(token: Tok) -> u32 {
    match token {
        // Reserved minimum precedence value is 1
        Tok::EqualEqualGreater => 2,
        Tok::LessEqualEqualGreater => 2,
        Tok::PipePipe => 3,
        Tok::AmpAmp => 4,
        Tok::EqualEqual => 5,
        Tok::ExclaimEqual => 5,
        Tok::Less => 5,
        Tok::Greater => 5,
        Tok::LessEqual => 5,
        Tok::GreaterEqual => 5,
        Tok::PeriodPeriod => 6,
        Tok::Pipe => 7,
        Tok::Caret => 8,
        Tok::Amp => 9,
        Tok::LessLess => 10,
        Tok::GreaterGreater => 10,
        Tok::Plus => 11,
        Tok::Minus => 11,
        Tok::Star => 12,
        Tok::Slash => 12,
        Tok::Percent => 12,
        _ => 0, // anything else is not a binary operator
    }
}

// Parse a binary operator expression:
//      BinOpExp =
//          <BinOpExp> <BinOp> <BinOpExp>
//          | <UnaryExp>
//      BinOp = (listed from lowest to highest precedence)
//          "==>"                                       spec only
//          | "||"
//          | "&&"
//          | "==" | "!=" | '<' | ">" | "<=" | ">="
//          | ".."                                      spec only
//          | "|"
//          | "^"
//          | "&"
//          | "<<" | ">>"
//          | "+" | "-"
//          | "*" | "/" | "%"
//
// This function takes the LHS of the expression as an argument, and it
// continues parsing binary expressions as long as they have at least the
// specified "min_prec" minimum precedence.
fn parse_binop_exp(context: &mut Context, lhs: Exp, min_prec: u32) -> Result<Exp, Box<Diagnostic>> {
    let mut result = lhs;
    let mut next_tok_prec = get_precedence(context.tokens.peek());

    while next_tok_prec >= min_prec {
        // Parse the operator.
        let op_start_loc = context.tokens.start_loc();
        let op_token = context.tokens.peek();
        context.tokens.advance()?;
        let op_end_loc = context.tokens.previous_end_loc();

        let mut rhs = parse_unary_exp(context)?;

        // If the next token is another binary operator with a higher
        // precedence, then recursively parse that expression as the RHS.
        let this_prec = next_tok_prec;
        next_tok_prec = get_precedence(context.tokens.peek());
        if this_prec < next_tok_prec {
            rhs = parse_binop_exp(context, rhs, this_prec + 1)?;
            next_tok_prec = get_precedence(context.tokens.peek());
        }

        let op = match op_token {
            Tok::EqualEqual => BinOp_::Eq,
            Tok::ExclaimEqual => BinOp_::Neq,
            Tok::Less => BinOp_::Lt,
            Tok::Greater => BinOp_::Gt,
            Tok::LessEqual => BinOp_::Le,
            Tok::GreaterEqual => BinOp_::Ge,
            Tok::PipePipe => BinOp_::Or,
            Tok::AmpAmp => BinOp_::And,
            Tok::Caret => BinOp_::Xor,
            Tok::Pipe => BinOp_::BitOr,
            Tok::Amp => BinOp_::BitAnd,
            Tok::LessLess => BinOp_::Shl,
            Tok::GreaterGreater => BinOp_::Shr,
            Tok::Plus => BinOp_::Add,
            Tok::Minus => BinOp_::Sub,
            Tok::Star => BinOp_::Mul,
            Tok::Slash => BinOp_::Div,
            Tok::Percent => BinOp_::Mod,
            Tok::PeriodPeriod => BinOp_::Range,
            Tok::EqualEqualGreater => BinOp_::Implies,
            Tok::LessEqualEqualGreater => BinOp_::Iff,
            _ => panic!("Unexpected token that is not a binary operator"),
        };
        let sp_op = spanned(context.tokens.file_hash(), op_start_loc, op_end_loc, op);

        let start_loc = result.loc.start() as usize;
        let end_loc = context.tokens.previous_end_loc();
        let e = Exp_::BinopExp(Box::new(result), sp_op, Box::new(rhs));
        result = spanned(context.tokens.file_hash(), start_loc, end_loc, e);
    }

    Ok(result)
}

// Parse a unary expression:
//      UnaryExp =
//          "!" <UnaryExp>
//          | "&mut" <UnaryExp>
//          | "&" "mut" <UnaryExp>
//          | "&" <UnaryExp>
//          | "*" <UnaryExp>
//          | "move" <UnaryExp>
//          | "copy" <UnaryExp>
//          | <DotOrIndexChain>
fn parse_unary_exp(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let exp = match context.tokens.peek() {
        Tok::Exclaim => {
            context.tokens.advance()?;
            let op_end_loc = context.tokens.previous_end_loc();
            let op = spanned(
                context.tokens.file_hash(),
                start_loc,
                op_end_loc,
                UnaryOp_::Not,
            );
            let e = parse_unary_exp(context)?;
            Exp_::UnaryExp(op, Box::new(e))
        }
        Tok::AmpMut => {
            context.tokens.advance()?;
            let e = parse_unary_exp(context)?;
            Exp_::Borrow(true, Box::new(e))
        }
        Tok::Amp => {
            context.tokens.advance()?;
            let is_mut = match_token(context.tokens, Tok::Mut)?;
            let e = parse_unary_exp(context)?;
            Exp_::Borrow(is_mut, Box::new(e))
        }
        Tok::Star => {
            context.tokens.advance()?;
            let e = parse_unary_exp(context)?;
            Exp_::Dereference(Box::new(e))
        }
        Tok::Move => {
            context.tokens.advance()?;
            let op_end_loc = make_loc(
                context.tokens.file_hash(),
                start_loc,
                context.tokens.previous_end_loc(),
            );
            let e = parse_unary_exp(context)?;
            Exp_::Move(op_end_loc, Box::new(e))
        }
        Tok::Copy => {
            context.tokens.advance()?;
            let op_end_loc = make_loc(
                context.tokens.file_hash(),
                start_loc,
                context.tokens.previous_end_loc(),
            );
            let e = parse_unary_exp(context)?;
            Exp_::Copy(op_end_loc, Box::new(e))
        }
        _ => {
            return parse_dot_or_index_chain(context);
        }
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, exp))
}

// Parse an expression term optionally followed by a chain of dot or index accesses:
//      DotOrIndexChain =
//          <DotOrIndexChain> "." <Identifier>
//          | <DotOrIndexChain> "." <Number>
//          | <DotOrIndexChain> "[" <Exp> "]"                      spec only
//          | <DotOrIndexChain> <OptionalTypeArgs> "(" Comma<Exp> ")"
//          | <Term>
fn parse_dot_or_index_chain(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let mut lhs = parse_term(context)?;
    loop {
        let exp = match context.tokens.peek() {
            Tok::Period => {
                context.tokens.advance()?;
                let loc = current_token_loc(context.tokens);
                match context.tokens.peek() {
                    Tok::NumValue | Tok::NumTypedValue
                        if context.env.check_feature(
                            FeatureGate::PositionalFields,
                            context.package_name,
                            loc,
                        ) =>
                    {
                        let contents = context.tokens.content();
                        context.tokens.advance()?;
                        match parse_u8(contents) {
                            Ok((parsed, NumberFormat::Decimal)) => {
                                let field_access = Name::new(loc, format!("{parsed}").into());
                                Exp_::Dot(Box::new(lhs), field_access)
                            }
                            Ok((_, NumberFormat::Hex)) => {
                                let msg = "Invalid field access. Expected a decimal number but was given a hexadecimal";
                                let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                                diag.add_note("Positional fields must be a decimal number in the range [0 .. 255] and not be typed, e.g. `0`");
                                context.env.add_diag(diag);
                                // Continue on with the parsing
                                let field_access = Name::new(loc, contents.into());
                                Exp_::Dot(Box::new(lhs), field_access)
                            }
                            Err(_) => {
                                let msg = format!(
                                    "Invalid field access. Expected a number less than or equal to {}",
                                    u8::MAX
                                );
                                let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                                diag.add_note("Positional fields must be a decimal number in the range [0 .. 255] and not be typed, e.g. `0`");
                                context.env.add_diag(diag);
                                // Continue on with the parsing
                                let field_access = Name::new(loc, contents.into());
                                Exp_::Dot(Box::new(lhs), field_access)
                            }
                        }
                    }
                    _ => {
                        let n = parse_identifier(context)?;
                        if is_start_of_call_after_function_name(context, &n) {
                            let call_start = context.tokens.start_loc();
                            let mut tys = None;
                            if context.tokens.peek() == Tok::Less
                                && n.loc.end() as usize == call_start
                            {
                                let loc =
                                    make_loc(context.tokens.file_hash(), call_start, call_start);
                                tys = parse_optional_type_args(context)
                                    .map_err(|diag| add_type_args_ambiguity_label(loc, diag))?;
                            }
                            let args = parse_call_args(context)?;
                            Exp_::DotCall(Box::new(lhs), n, tys, args)
                        } else {
                            Exp_::Dot(Box::new(lhs), n)
                        }
                    }
                }
            }
            Tok::LBracket => {
                context.tokens.advance()?;
                let index = parse_exp(context)?;
                let exp = Exp_::Index(Box::new(lhs), Box::new(index));
                consume_token(context.tokens, Tok::RBracket)?;
                exp
            }
            _ => break,
        };
        let end_loc = context.tokens.previous_end_loc();
        lhs = spanned(context.tokens.file_hash(), start_loc, end_loc, exp);
    }
    Ok(lhs)
}

// Look ahead to determine if this is the start of a call expression. Used when parsing method calls
// to determine if we should parse the type arguments and args following a name. Otherwise, we will
// parse a field access
fn is_start_of_call_after_function_name(context: &Context, n: &Name) -> bool {
    // TODO(macro) consider macro Tok::Exlaim
    let call_start = context.tokens.start_loc();
    let peeked = context.tokens.peek();
    (peeked == Tok::Less && n.loc.end() as usize == call_start) || peeked == Tok::LParen
}

// Lookahead to determine whether this is a quantifier. This matches
//
//      ( "exists" | "forall" | "choose" | "min" )
//          <Identifier> ( ":" | <Identifier> ) ...
//
// as a sequence to identify a quantifier. While the <Identifier> after
// the exists/forall would by syntactically sufficient (Move does not
// have affixed identifiers in expressions), we add another token
// of lookahead to keep the result more precise in the presence of
// syntax errors.
fn is_quant(context: &mut Context) -> bool {
    if !matches!(context.tokens.content(), "exists" | "forall" | "choose") {
        return false;
    }
    match context.tokens.lookahead2() {
        Err(_) => false,
        Ok((tok1, tok2)) => tok1 == Tok::Identifier && matches!(tok2, Tok::Colon | Tok::Identifier),
    }
}

// Parses a quantifier expressions, assuming is_quant(context) is true.
//
//   <Quantifier> =
//       ( "forall" | "exists" ) <QuantifierBindings> ({ (<Exp>)* })* ("where" <Exp>)? ":" Exp
//     | ( "choose" [ "min" ] ) <QuantifierBind> "where" <Exp>
//   <QuantifierBindings> = <QuantifierBind> ("," <QuantifierBind>)*
//   <QuantifierBind> = <Identifier> ":" <Type> | <Identifier> "in" <Exp>
//
fn parse_quant(context: &mut Context) -> Result<Exp_, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let kind = match context.tokens.content() {
        "exists" => {
            context.tokens.advance()?;
            QuantKind_::Exists
        }
        "forall" => {
            context.tokens.advance()?;
            QuantKind_::Forall
        }
        "choose" => {
            context.tokens.advance()?;
            match context.tokens.peek() {
                Tok::Identifier if context.tokens.content() == "min" => {
                    context.tokens.advance()?;
                    QuantKind_::ChooseMin
                }
                _ => QuantKind_::Choose,
            }
        }
        _ => unreachable!(),
    };
    let spanned_kind = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        kind,
    );

    if matches!(kind, QuantKind_::Choose | QuantKind_::ChooseMin) {
        let binding = parse_quant_binding(context)?;
        consume_identifier(context.tokens, "where")?;
        let body = parse_exp(context)?;
        return Ok(Exp_::Quant(
            spanned_kind,
            Spanned {
                loc: binding.loc,
                value: vec![binding],
            },
            vec![],
            None,
            Box::new(body),
        ));
    }

    let bindings_start_loc = context.tokens.start_loc();
    let binds_with_range_list = parse_list(
        context,
        |context| {
            if context.tokens.peek() == Tok::Comma {
                context.tokens.advance()?;
                Ok(true)
            } else {
                Ok(false)
            }
        },
        parse_quant_binding,
    )?;
    let binds_with_range_list = spanned(
        context.tokens.file_hash(),
        bindings_start_loc,
        context.tokens.previous_end_loc(),
        binds_with_range_list,
    );

    let triggers = if context.tokens.peek() == Tok::LBrace {
        parse_list(
            context,
            |context| {
                if context.tokens.peek() == Tok::LBrace {
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
            |context| {
                parse_comma_list(
                    context,
                    Tok::LBrace,
                    Tok::RBrace,
                    parse_exp,
                    "a trigger expresssion",
                )
            },
        )?
    } else {
        Vec::new()
    };

    let condition = match context.tokens.peek() {
        Tok::Identifier if context.tokens.content() == "where" => {
            context.tokens.advance()?;
            Some(Box::new(parse_exp(context)?))
        }
        _ => None,
    };
    consume_token(context.tokens, Tok::Colon)?;
    let body = parse_exp(context)?;

    Ok(Exp_::Quant(
        spanned_kind,
        binds_with_range_list,
        triggers,
        condition,
        Box::new(body),
    ))
}

// Parses one quantifier binding.
fn parse_quant_binding(context: &mut Context) -> Result<Spanned<(Bind, Exp)>, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let ident = parse_identifier(context)?;
    let bind = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        Bind_::Var(None, Var(ident)),
    );
    let range = if context.tokens.peek() == Tok::Colon {
        // This is a quantifier over the full domain of a type.
        // Built `domain<ty>()` expression.
        context.tokens.advance()?;
        let ty = parse_type(context)?;
        make_builtin_call(ty.loc, symbol!("$spec_domain"), Some(vec![ty]), vec![])
    } else {
        // This is a quantifier over a value, like a vector or a range.
        consume_identifier(context.tokens, "in")?;
        parse_exp(context)?
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        (bind, range),
    ))
}

fn make_builtin_call(loc: Loc, name: Symbol, type_args: Option<Vec<Type>>, args: Vec<Exp>) -> Exp {
    let maccess = sp(loc, NameAccessChain_::One(sp(loc, name)));
    sp(loc, Exp_::Call(maccess, false, type_args, sp(loc, args)))
}

//**************************************************************************************************
// Types
//**************************************************************************************************

// Parse a Type:
//      Type =
//          <NameAccessChain> ('<' Comma<Type> ">")?
//          | "&" <Type>
//          | "&mut" <Type>
//          | "&" "mut" <Type>
//          | "|" Comma<Type> "|" Type   (spec only)
//          | "(" Comma<Type> ")"
fn parse_type(context: &mut Context) -> Result<Type, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let t = match context.tokens.peek() {
        Tok::LParen => {
            let mut ts = parse_comma_list(context, Tok::LParen, Tok::RParen, parse_type, "a type")?;
            match ts.len() {
                0 => Type_::Unit,
                1 => ts.pop().unwrap().value,
                _ => Type_::Multiple(ts),
            }
        }
        Tok::Amp => {
            context.tokens.advance()?;
            let is_mut = match_token(context.tokens, Tok::Mut)?;
            let t = parse_type(context)?;
            Type_::Ref(is_mut, Box::new(t))
        }
        Tok::AmpMut => {
            context.tokens.advance()?;
            let t = parse_type(context)?;
            Type_::Ref(true, Box::new(t))
        }
        Tok::Pipe => {
            let args = parse_comma_list(context, Tok::Pipe, Tok::Pipe, parse_type, "a type")?;
            let result = parse_type(context)?;
            return Ok(spanned(
                context.tokens.file_hash(),
                start_loc,
                context.tokens.previous_end_loc(),
                Type_::Fun(args, Box::new(result)),
            ));
        }
        _ => {
            let tn = parse_name_access_chain(context, || "a type name")?;
            let tys = if context.tokens.peek() == Tok::Less {
                parse_comma_list(context, Tok::Less, Tok::Greater, parse_type, "a type")?
            } else {
                vec![]
            };
            Type_::Apply(Box::new(tn), tys)
        }
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, t))
}

// Parse an optional list of type arguments.
//    OptionalTypeArgs = '<' Comma<Type> ">" | <empty>
fn parse_optional_type_args(context: &mut Context) -> Result<Option<Vec<Type>>, Box<Diagnostic>> {
    if context.tokens.peek() == Tok::Less {
        Ok(Some(parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            parse_type,
            "a type",
        )?))
    } else {
        Ok(None)
    }
}

fn token_to_ability(token: Tok, content: &str) -> Option<Ability_> {
    match (token, content) {
        (Tok::Copy, _) => Some(Ability_::Copy),
        (Tok::Identifier, Ability_::DROP) => Some(Ability_::Drop),
        (Tok::Identifier, Ability_::STORE) => Some(Ability_::Store),
        (Tok::Identifier, Ability_::KEY) => Some(Ability_::Key),
        _ => None,
    }
}

// Parse a type ability
//      Ability =
//          <Copy>
//          | "drop"
//          | "store"
//          | "key"
fn parse_ability(context: &mut Context) -> Result<Ability, Box<Diagnostic>> {
    let loc = current_token_loc(context.tokens);
    match token_to_ability(context.tokens.peek(), context.tokens.content()) {
        Some(ability) => {
            context.tokens.advance()?;
            Ok(sp(loc, ability))
        }
        None => {
            let msg = format!(
                "Unexpected {}. Expected a type ability, one of: 'copy', 'drop', 'store', or 'key'",
                current_token_error_string(context.tokens)
            );
            Err(Box::new(diag!(Syntax::UnexpectedToken, (loc, msg))))
        }
    }
}

// Parse a type parameter:
//      TypeParameter =
//          <Identifier> <Constraint>?
//      Constraint =
//          ":" <Ability> (+ <Ability>)*
fn parse_type_parameter(context: &mut Context) -> Result<(Name, Vec<Ability>), Box<Diagnostic>> {
    let n = parse_identifier(context)?;

    let ability_constraints = if match_token(context.tokens, Tok::Colon)? {
        parse_list(
            context,
            |context| match context.tokens.peek() {
                Tok::Plus => {
                    context.tokens.advance()?;
                    Ok(true)
                }
                Tok::Greater | Tok::Comma => Ok(false),
                _ => Err(unexpected_token_error(
                    context.tokens,
                    &format!(
                        "one of: '{}', '{}', or '{}'",
                        Tok::Plus,
                        Tok::Greater,
                        Tok::Comma
                    ),
                )),
            },
            parse_ability,
        )?
    } else {
        vec![]
    };
    Ok((n, ability_constraints))
}

// Parse type parameter with optional phantom declaration:
//   TypeParameterWithPhantomDecl = "phantom"? <TypeParameter>
fn parse_type_parameter_with_phantom_decl(
    context: &mut Context,
) -> Result<(bool, Name, Vec<Ability>), Box<Diagnostic>> {
    let is_phantom =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "phantom" {
            context.tokens.advance()?;
            true
        } else {
            false
        };
    let (name, constraints) = parse_type_parameter(context)?;
    Ok((is_phantom, name, constraints))
}

// Parse optional type parameter list.
//    OptionalTypeParameters = '<' Comma<TypeParameter> ">" | <empty>
fn parse_optional_type_parameters(
    context: &mut Context,
) -> Result<Vec<(Name, Vec<Ability>)>, Box<Diagnostic>> {
    if context.tokens.peek() == Tok::Less {
        parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            parse_type_parameter,
            "a type parameter",
        )
    } else {
        Ok(vec![])
    }
}

// Parse optional struct type parameters:
//    StructTypeParameter = '<' Comma<TypeParameterWithPhantomDecl> ">" | <empty>
fn parse_struct_type_parameters(
    context: &mut Context,
) -> Result<Vec<DatatypeTypeParameter>, Box<Diagnostic>> {
    if context.tokens.peek() == Tok::Less {
        parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            parse_struct_type_parameter,
            "a type parameter",
        )
    } else {
        Ok(vec![])
    }
}

// Parse type parameter with optional phantom declaration:
//   TypeParameterWithPhantomDecl = "phantom"? <TypeParameter>
fn parse_struct_type_parameter(
    context: &mut Context,
) -> Result<DatatypeTypeParameter, Box<Diagnostic>> {
    let (is_phantom, name, constraints) = parse_type_parameter_with_phantom_decl(context)?;
    Ok(DatatypeTypeParameter {
        is_phantom,
        name,
        constraints,
    })
}

// Parse optional enum type parameters:
//    EnumTypeParameter = '<' Comma<TypeParameterWithPhantomDecl> ">" | <empty>
fn parse_enum_type_parameters(
    context: &mut Context,
) -> Result<Vec<DatatypeTypeParameter>, Box<Diagnostic>> {
    if context.tokens.peek() == Tok::Less {
        parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            parse_enum_type_parameter,
            "a type parameter",
        )
    } else {
        Ok(vec![])
    }
}

// Parse type parameter with optional phantom declaration:
//   TypeParameterWithPhantomDecl = "phantom"? <TypeParameter>
fn parse_enum_type_parameter(
    context: &mut Context,
) -> Result<DatatypeTypeParameter, Box<Diagnostic>> {
    let (is_phantom, name, constraints) = parse_type_parameter_with_phantom_decl(context)?;
    Ok(DatatypeTypeParameter {
        is_phantom,
        name,
        constraints,
    })
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

// Parse a function declaration:
//      FunctionDecl =
//          "fun"
//          <FunctionDefName> "(" Comma<Parameter> ")"
//          (":" <Type>)?
//          ("acquires" <NameAccessChain> ("," <NameAccessChain>)*)?
//          ("{" <Sequence> "}" | ";")
//
fn parse_function_decl(
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<Function, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
    } = modifiers;

    // "fun" <FunctionDefName>
    consume_token(context.tokens, Tok::Fun)?;
    let name = FunctionName(parse_identifier(context)?);
    let type_parameters = parse_optional_type_parameters(context)?;

    // "(" Comma<Parameter> ")"
    let parameters = parse_comma_list(
        context,
        Tok::LParen,
        Tok::RParen,
        parse_parameter,
        "a function parameter",
    )?;

    // (":" <Type>)?
    let return_type = if match_token(context.tokens, Tok::Colon)? {
        parse_type(context)?
    } else {
        sp(name.loc(), Type_::Unit)
    };

    // ("acquires" (<NameAccessChain> ",")* <NameAccessChain> ","?
    let mut acquires = vec![];
    if match_token(context.tokens, Tok::Acquires)? {
        let follows_acquire = |tok| matches!(tok, Tok::Semicolon | Tok::LBrace);
        loop {
            acquires.push(parse_name_access_chain(context, || {
                "a resource struct name"
            })?);
            if follows_acquire(context.tokens.peek()) {
                break;
            }
            consume_token(context.tokens, Tok::Comma)?;
            if follows_acquire(context.tokens.peek()) {
                break;
            }
        }
    }

    let body = match native {
        Some(loc) => {
            consume_token(context.tokens, Tok::Semicolon)?;
            sp(loc, FunctionBody_::Native)
        }
        _ => {
            let start_loc = context.tokens.start_loc();
            consume_token(context.tokens, Tok::LBrace)?;
            let seq = parse_sequence(context)?;
            let end_loc = context.tokens.previous_end_loc();
            sp(
                make_loc(context.tokens.file_hash(), start_loc, end_loc),
                FunctionBody_::Defined(seq),
            )
        }
    };

    let signature = FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    };

    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(Function {
        attributes,
        loc,
        visibility: visibility.unwrap_or(Visibility::Internal),
        entry,
        signature,
        name,
        body,
    })
}

// Parse a function parameter:
//      Parameter = "mut"? <Var> ":" <Type>
fn parse_parameter(context: &mut Context) -> Result<(Mutability, Var, Type), Box<Diagnostic>> {
    let mut_ = if context.tokens.peek() == Tok::Mut {
        let start_loc = context.tokens.start_loc();
        context.tokens.advance()?;
        let end_loc = context.tokens.previous_end_loc();
        Some(make_loc(context.tokens.file_hash(), start_loc, end_loc))
    } else {
        None
    };
    let v = parse_var(context)?;
    consume_token(context.tokens, Tok::Colon)?;
    let t = parse_type(context)?;
    Ok((mut_, v, t))
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

// Parse an enum definition:
//      EnumDecl =
//          "enum" <EnumDefName> ("has" <Ability> (, <Ability>)+)?
//          "{" (<VariantDecl>,)+ "}" ("has" <Ability> (, <Ability>)+;)
//      EnumDefName =
//          <Identifier> <OptionalTypeParameters>
// Where the the two "has" statements are mutually exclusive -- an enum cannot be declared with
// both infix and postfix ability declarations.

fn parse_enum_decl(
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<EnumDefinition, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native: _,
    } = modifiers;

    check_enum_visibility(visibility, context);

    if let Some(loc) = entry {
        let msg = format!(
            "Invalid enum declaration. '{}' is used only on functions",
            ENTRY_MODIFIER
        );
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }

    consume_token(context.tokens, Tok::Enum)?;

    // <EnumDefName>
    let name = DatatypeName(parse_identifier(context)?);
    let type_parameters = parse_enum_type_parameters(context)?;

    let infix_ability_declaration_loc =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "has" {
            Some(current_token_loc(context.tokens))
        } else {
            None
        };
    let mut abilities = if infix_ability_declaration_loc.is_some() {
        context.tokens.advance()?;
        parse_list(
            context,
            |context| match context.tokens.peek() {
                Tok::Comma => {
                    context.tokens.advance()?;
                    Ok(true)
                }
                Tok::LBrace | Tok::Semicolon | Tok::LParen => Ok(false),
                _ => Err(unexpected_token_error(
                    context.tokens,
                    &format!(
                        "one of: '{}', '{}', '{}', or '{}'",
                        Tok::Comma,
                        Tok::LBrace,
                        Tok::LParen,
                        Tok::Semicolon
                    ),
                )),
            },
            parse_ability,
        )?
    } else {
        vec![]
    };

    let variants = parse_enum_variant_decls(context)?;
    parse_postfix_ability_declarations(infix_ability_declaration_loc, &mut abilities, context)?;

    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(EnumDefinition {
        attributes,
        loc,
        abilities,
        name,
        type_parameters,
        variants,
    })
}

fn parse_enum_variant_decls(
    context: &mut Context,
) -> Result<Vec<VariantDefinition>, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    expect_token!(
        context.tokens,
        Tok::LBrace,
        Tok::LParen =>
            (Syntax::UnexpectedToken, context.tokens.current_token_loc(), "Enum variants must be within '{}' blocks"),
        Tok::Semicolon =>
            (Syntax::UnexpectedToken, context.tokens.current_token_loc(), "Native enums are not supported")
    )?;

    let variants = parse_comma_list_after_start(
        context,
        start_loc,
        Tok::LBrace,
        Tok::RBrace,
        parse_enum_variant_decl,
        "a variant",
    )?;
    Ok(variants)
}

// Parse an enum variant definition:
//      VariantDecl = <Identifier> ("{" Comma<FieldAnnot> "}" | "(" Comma<PosField> ")")
fn parse_enum_variant_decl(context: &mut Context) -> Result<VariantDefinition, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let name = parse_identifier(context)?;
    let fields = parse_enum_variant_fields(context)?;
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(VariantDefinition {
        loc,
        name: VariantName(name),
        fields,
    })
}

fn parse_enum_variant_fields(context: &mut Context) -> Result<VariantFields, Box<Diagnostic>> {
    match context.tokens.peek() {
        Tok::LParen => {
            let current_package = context.package_name;
            let loc = current_token_loc(context.tokens);
            context
                .env
                .check_feature(FeatureGate::PositionalFields, current_package, loc);

            let list = parse_comma_list(
                context,
                Tok::LParen,
                Tok::RParen,
                parse_positional_field,
                "a type",
            )?;
            Ok(VariantFields::Positional(list))
        }
        Tok::LBrace => {
            let fields = parse_comma_list(
                context,
                Tok::LBrace,
                Tok::RBrace,
                parse_field_annot,
                "a field",
            )?;
            Ok(VariantFields::Named(fields))
        }
        _ => Ok(VariantFields::Empty),
    }
}

fn check_enum_visibility(visibility: Option<Visibility>, context: &mut Context) {
    let current_package = context.package_name;
    // NB this could be an if-let but we will eventually want the match for other vis. support.
    match &visibility {
        Some(Visibility::Public(loc)) => {
            context
                .env
                .check_feature(FeatureGate::Enums, current_package, *loc);
        }
        vis => {
            let (loc, vis_str) = match vis {
                Some(vis) => (vis.loc().unwrap(), format!("'{vis}'")),
                None => {
                    let loc = current_token_loc(context.tokens);
                    (loc, "Internal".to_owned())
                }
            };
            let msg = format!(
                "Invalid enum declaration. {vis_str} enum declarations are not yet supported"
            );
            let note = "Visibility annotations are required on enum declarations.";
            let mut err = diag!(Syntax::InvalidModifier, (loc, msg));
            err.add_note(note);
            context.env.add_diag(err);
        }
    }
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

// Parse a struct definition:
//      StructDecl =
//          "struct" <StructDefName> ("has" <Ability> (, <Ability>)+)?
//          (("{" Comma<FieldAnnot> "}" | "(" Comma<PosField> ")") ("has" <Ability> (, <Ability>)+;)? | ";")
//      StructDefName =
//          <Identifier> <OptionalTypeParameters>
// Where the the two "has" statements are mutually exclusive -- a struct cannot be declared with
// both infix and postfix ability declarations.
fn parse_struct_decl(
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<StructDefinition, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
    } = modifiers;

    check_struct_visibility(visibility, context);

    if let Some(loc) = entry {
        let msg = format!(
            "Invalid struct declaration. '{}' is used only on functions",
            ENTRY_MODIFIER
        );
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }

    consume_token(context.tokens, Tok::Struct)?;

    // <StructDefName>
    let name = DatatypeName(parse_identifier(context)?);
    let type_parameters = parse_struct_type_parameters(context)?;

    let infix_ability_declaration_loc =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "has" {
            Some(current_token_loc(context.tokens))
        } else {
            None
        };
    let mut abilities = if infix_ability_declaration_loc.is_some() {
        context.tokens.advance()?;
        parse_list(
            context,
            |context| match context.tokens.peek() {
                Tok::Comma => {
                    context.tokens.advance()?;
                    Ok(true)
                }
                Tok::LBrace | Tok::Semicolon | Tok::LParen => Ok(false),
                _ => Err(unexpected_token_error(
                    context.tokens,
                    &format!(
                        "one of: '{}', '{}', '{}', or '{}'",
                        Tok::Comma,
                        Tok::LBrace,
                        Tok::LParen,
                        Tok::Semicolon
                    ),
                )),
            },
            parse_ability,
        )?
    } else {
        vec![]
    };

    let fields = match native {
        Some(loc) => {
            consume_token(context.tokens, Tok::Semicolon)?;
            StructFields::Native(loc)
        }
        _ => {
            let fields = parse_struct_fields(context)?;
            parse_postfix_ability_declarations(
                infix_ability_declaration_loc,
                &mut abilities,
                context,
            )?;
            fields
        }
    };

    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(StructDefinition {
        attributes,
        loc,
        abilities,
        name,
        type_parameters,
        fields,
    })
}

// Parse a field annotated with a type:
//      FieldAnnot = <DocComments> <Field> ":" <Type>
fn parse_field_annot(context: &mut Context) -> Result<(Field, Type), Box<Diagnostic>> {
    context.tokens.match_doc_comments();
    let f = parse_field(context)?;
    consume_token(context.tokens, Tok::Colon)?;
    let st = parse_type(context)?;
    Ok((f, st))
}

// Parse a positional struct field:
//      PosField = <DocComments> <Type>
fn parse_positional_field(context: &mut Context) -> Result<Type, Box<Diagnostic>> {
    context.tokens.match_doc_comments();
    if matches!(
        (context.tokens.peek(), context.tokens.lookahead()),
        (Tok::Identifier, Ok(Tok::Colon))
    ) {
        return Err(Box::new(diag!(
            Syntax::UnexpectedToken,
            (
                context.tokens.current_token_loc(),
                "Cannot use named fields here"
            )
        )));
    }
    parse_type(context)
}

// Parse a postfix ability declaration:
//     "has" <Ability> (, <Ability>)+;
//  Error if:
//      * Also has prefix ability declaration
fn parse_postfix_ability_declarations(
    infix_ability_declaration_loc: Option<Loc>,
    abilities: &mut Vec<Ability>,
    context: &mut Context,
) -> Result<(), Box<Diagnostic>> {
    let postfix_ability_declaration =
        context.tokens.peek() == Tok::Identifier && context.tokens.content() == "has";
    let has_location = current_token_loc(context.tokens);

    if postfix_ability_declaration {
        context.env.check_feature(
            FeatureGate::PostFixAbilities,
            context.package_name,
            has_location,
        );

        context.tokens.advance()?;

        // Only add a diagnostic about prefix xor postfix ability declarations if the feature is
        // supported. Otherwise we will already have an error that the `has` is not supported in
        // that position, and the feature check diagnostic as well, so adding this additional error
        // could be confusing.
        if let Some(previous_declaration_loc) = infix_ability_declaration_loc {
            let msg = "Duplicate ability declaration. Abilities can be declared before \
                       or after the field declarations, but not both.";
            let prev_msg = "Ability declaration previously given here";
            context.env.add_diag(diag!(
                Syntax::InvalidModifier,
                (has_location, msg),
                (previous_declaration_loc, prev_msg)
            ));
        }

        *abilities = parse_list(
            context,
            |context| match context.tokens.peek() {
                Tok::Comma => {
                    context.tokens.advance()?;
                    Ok(true)
                }
                Tok::Semicolon => Ok(false),
                _ => Err(unexpected_token_error(
                    context.tokens,
                    &format!("one of: '{}' or '{}'", Tok::Comma, Tok::Semicolon),
                )),
            },
            parse_ability,
        )?;
        consume_token(context.tokens, Tok::Semicolon)?;
    }
    Ok(())
}

fn parse_struct_fields(context: &mut Context) -> Result<StructFields, Box<Diagnostic>> {
    let positional_declaration = context.tokens.peek() == Tok::LParen;
    if positional_declaration {
        let current_package = context.package_name;
        let loc = current_token_loc(context.tokens);
        context
            .env
            .check_feature(FeatureGate::PositionalFields, current_package, loc);

        let list = parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            parse_positional_field,
            "a type",
        )?;
        Ok(StructFields::Positional(list))
    } else {
        let fields = parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            parse_field_annot,
            "a field",
        )?;
        Ok(StructFields::Named(fields))
    }
}

fn check_struct_visibility(visibility: Option<Visibility>, context: &mut Context) {
    let current_package = context.package_name;
    if let Some(Visibility::Public(loc)) = &visibility {
        context
            .env
            .check_feature(FeatureGate::StructTypeVisibility, current_package, *loc);
    }

    let supports_public = context
        .env
        .supports_feature(current_package, FeatureGate::StructTypeVisibility);

    if supports_public {
        if !matches!(visibility, Some(Visibility::Public(_))) {
            let (loc, vis_str) = match visibility {
                Some(vis) => (vis.loc().unwrap(), format!("'{vis}'")),
                None => {
                    let loc = current_token_loc(context.tokens);
                    (loc, "Internal".to_owned())
                }
            };
            let msg = format!(
                "Invalid struct declaration. {vis_str} struct declarations are not yet supported"
            );
            let note = "Visibility annotations are required on struct declarations from the Move 2024 edition onwards.";
            let mut err = diag!(Syntax::InvalidModifier, (loc, msg));
            err.add_note(note);
            context.env.add_diag(err);
        }
    } else if let Some(vis) = visibility {
        let msg = format!(
            "Invalid struct declaration. Structs cannot have visibility modifiers as they are \
                always '{}'",
            Visibility::PUBLIC
        );
        let note = "Starting in the Move 2024 edition visibility must be annotated on struct declarations.";
        let mut err = diag!(Syntax::InvalidModifier, (vis.loc().unwrap(), msg));
        err.add_note(note);
        context.env.add_diag(err);
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

// Parse a constant:
//      ConstantDecl = "const" <Identifier> ":" <Type> "=" <Exp> ";"
fn parse_constant_decl(
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<Constant, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
    } = modifiers;
    if let Some(vis) = visibility {
        let msg = "Invalid constant declaration. Constants cannot have visibility modifiers as \
                   they are always internal";
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (vis.loc().unwrap(), msg)));
    }
    if let Some(loc) = entry {
        let msg = format!(
            "Invalid constant declaration. '{}' is used only on functions",
            ENTRY_MODIFIER
        );
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }
    if let Some(loc) = native {
        let msg = "Invalid constant declaration. 'native' constants are not supported";
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }
    consume_token(context.tokens, Tok::Const)?;
    let name = ConstantName(parse_identifier(context)?);
    expect_token!(
        context.tokens,
        Tok::Colon,
        Tok::Equal =>
        (
            Syntax::UnexpectedToken,
            context.tokens.current_token_loc(),
            format!("Expected a type annotation for this constant, e.g. '{}: <type>'", name)
        )
    )?;
    let signature = parse_type(context)?;
    consume_token(context.tokens, Tok::Equal)?;
    let value = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(Constant {
        attributes,
        loc,
        signature,
        name,
        value,
    })
}

//**************************************************************************************************
// AddressBlock
//**************************************************************************************************

// Parse an address block:
//      AddressBlock =
//          "address" <LeadingNameAccess> "{" (<Attributes> <Module>)* "}"
//
// Note that "address" is not a token.
fn parse_address_block(
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<AddressDefinition, Box<Diagnostic>> {
    const UNEXPECTED_TOKEN: &str = "Invalid code unit. Expected 'address' or 'module'";
    if context.tokens.peek() != Tok::Identifier {
        let start = context.tokens.start_loc();
        let end = start + context.tokens.content().len();
        let loc = make_loc(context.tokens.file_hash(), start, end);
        let msg = format!(
            "{}. Got {}",
            UNEXPECTED_TOKEN,
            current_token_error_string(context.tokens)
        );
        return Err(Box::new(diag!(Syntax::UnexpectedToken, (loc, msg))));
    }
    let addr_name = parse_identifier(context)?;
    if addr_name.value != symbol!("address") {
        let msg = format!("{}. Got '{}'", UNEXPECTED_TOKEN, addr_name.value);
        return Err(Box::new(diag!(
            Syntax::UnexpectedToken,
            (addr_name.loc, msg)
        )));
    }
    let start_loc = context.tokens.start_loc();
    let addr = parse_leading_name_access(context)?;
    let end_loc = context.tokens.previous_end_loc();
    let loc = make_loc(context.tokens.file_hash(), start_loc, end_loc);

    let modules = match context.tokens.peek() {
        Tok::LBrace => {
            context.tokens.advance()?;
            let mut modules = vec![];
            while context.tokens.peek() != Tok::RBrace {
                let attributes = parse_attributes(context)?;
                modules.push(parse_module(attributes, context)?);
            }
            consume_token(context.tokens, Tok::RBrace)?;
            modules
        }
        _ => return Err(unexpected_token_error(context.tokens, "'{'")),
    };

    Ok(AddressDefinition {
        attributes,
        loc,
        addr,
        modules,
    })
}

//**************************************************************************************************
// Friends
//**************************************************************************************************

// Parse a friend declaration:
//      FriendDecl =
//          "friend" <NameAccessChain> ";"
fn parse_friend_decl(
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<FriendDecl, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, Tok::Friend)?;
    let friend = parse_name_access_chain(context, || "a friend declaration")?;
    consume_token(context.tokens, Tok::Semicolon)?;
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(FriendDecl {
        attributes,
        loc,
        friend,
    })
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

// Parse a use declaration:
//      UseDecl =
//          "use" "fun" <NameAccessChain> "as" <Type> "." <Identifier> ";" |
//          "use" <LeadingNameAccess> "::" "{" <Comma<UseModule>> "}" ";" |
//          "use" <LeadingNameAccess> "::" <UseModule>> ";"
fn parse_use_decl(
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<UseDecl, Box<Diagnostic>> {
    consume_token(context.tokens, Tok::Use)?;
    let Modifiers {
        visibility,
        entry,
        native,
    } = modifiers;
    if let Some(loc) = entry {
        let msg = format!(
            "Invalid use declaration. '{}' is used only on functions",
            ENTRY_MODIFIER
        );
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }
    if let Some(loc) = native {
        let msg = "Invalid use declaration. Unexpected 'native' modifier";
        context
            .env
            .add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
    }
    let use_ = match context.tokens.peek() {
        Tok::Fun => {
            consume_token(context.tokens, Tok::Fun).unwrap();
            let function = parse_name_access_chain(context, || "a function name")?;
            consume_token(context.tokens, Tok::As)?;
            let ty = parse_name_access_chain(context, || "a type name")?;
            consume_token(context.tokens, Tok::Period)?;
            let method = parse_identifier(context)?;
            Use::Fun {
                visibility: visibility.unwrap_or(Visibility::Internal),
                function: Box::new(function),
                ty: Box::new(ty),
                method,
            }
        }
        _ => {
            if let Some(vis) = visibility {
                let msg =
                    "Invalid use declaration. Non-'use fun' declarations cannot have visibility \
                           modifiers as they are always internal";
                context
                    .env
                    .add_diag(diag!(Syntax::InvalidModifier, (vis.loc().unwrap(), msg)));
            }
            let address_start_loc = context.tokens.start_loc();
            let address = parse_leading_name_access(context)?;
            consume_token_(
                context.tokens,
                Tok::ColonColon,
                start_loc,
                " after an address in a use declaration",
            )?;
            match context.tokens.peek() {
                Tok::LBrace => {
                    let parse_inner = |ctxt: &mut Context<'_, '_, '_>| {
                        let (name, _, use_) = parse_use_module(ctxt)?;
                        Ok((name, use_))
                    };
                    let use_decls = parse_comma_list(
                        context,
                        Tok::LBrace,
                        Tok::RBrace,
                        parse_inner,
                        "a module use clause",
                    )?;
                    Use::NestedModuleUses(address, use_decls)
                }
                _ => {
                    let (name, end_loc, use_) = parse_use_module(context)?;
                    let loc = make_loc(context.tokens.file_hash(), address_start_loc, end_loc);
                    let module_ident = sp(
                        loc,
                        ModuleIdent_ {
                            address,
                            module: name,
                        },
                    );
                    Use::ModuleUse(module_ident, use_)
                }
            }
        }
    };
    consume_token(context.tokens, Tok::Semicolon)?;
    let end_loc = context.tokens.previous_end_loc();
    let loc = make_loc(context.tokens.file_hash(), start_loc, end_loc);
    Ok(UseDecl {
        attributes,
        loc,
        use_,
    })
}

// Parse a use declaration member:
//      UseModule =
//          <ModuleName> <UseAlias> |
//          <ModuleName> "::" <UseMember> |
//          <ModuleName> "::" "{" Comma<UseMember> "}"
fn parse_use_module(
    context: &mut Context,
) -> Result<(ModuleName, usize, ModuleUse), Box<Diagnostic>> {
    let module_name = parse_module_name(context)?;
    let end_loc = context.tokens.previous_end_loc();
    let alias_opt = parse_use_alias(context)?;
    let module_use = match (&alias_opt, context.tokens.peek()) {
        (None, Tok::ColonColon) => {
            consume_token(context.tokens, Tok::ColonColon)?;
            let sub_uses = match context.tokens.peek() {
                Tok::LBrace => parse_comma_list(
                    context,
                    Tok::LBrace,
                    Tok::RBrace,
                    parse_use_member,
                    "a module member alias",
                )?,
                _ => vec![parse_use_member(context)?],
            };
            ModuleUse::Members(sub_uses)
        }
        _ => ModuleUse::Module(alias_opt.map(ModuleName)),
    };
    Ok((module_name, end_loc, module_use))
}

// Parse an alias for a module member:
//      UseMember = <Identifier> <UseAlias>
fn parse_use_member(context: &mut Context) -> Result<(Name, Option<Name>), Box<Diagnostic>> {
    let member = parse_identifier(context)?;
    let alias_opt = parse_use_alias(context)?;
    Ok((member, alias_opt))
}

// Parse an 'as' use alias:
//      UseAlias = ("as" <Identifier>)?
fn parse_use_alias(context: &mut Context) -> Result<Option<Name>, Box<Diagnostic>> {
    Ok(if context.tokens.peek() == Tok::As {
        context.tokens.advance()?;
        Some(parse_identifier(context)?)
    } else {
        None
    })
}

// Parse a module:
//      Module =
//          <DocComments> ( "spec" | "module") (<LeadingNameAccess>::)?<ModuleName> "{"
//              ( <Attributes>
//                  ( <FriendDecl> | <SpecBlock> |
//                    <DocComments> <ModuleMemberModifiers>
//                        (<ConstantDecl> | <StructDecl> | <FunctionDecl> | <UseDecl>) )
//                  )
//              )*
//          "}"
fn parse_module(
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<ModuleDefinition, Box<Diagnostic>> {
    context.tokens.match_doc_comments();
    let start_loc = context.tokens.start_loc();

    let is_spec_module = if context.tokens.peek() == Tok::Spec {
        context.tokens.advance()?;
        true
    } else {
        consume_token(context.tokens, Tok::Module)?;
        false
    };
    let sp!(n1_loc, n1_) = parse_leading_name_access(context)?;
    let (address, name) = match (n1_, context.tokens.peek()) {
        (addr_ @ LeadingNameAccess_::AnonymousAddress(_), _)
        | (addr_ @ LeadingNameAccess_::GlobalAddress(_), _)
        | (addr_ @ LeadingNameAccess_::Name(_), Tok::ColonColon) => {
            let addr = sp(n1_loc, addr_);
            consume_token(context.tokens, Tok::ColonColon)?;
            let name = parse_module_name(context)?;
            (Some(addr), name)
        }
        (LeadingNameAccess_::Name(name), _) => (None, ModuleName(name)),
    };
    consume_token(context.tokens, Tok::LBrace)?;

    let mut members = vec![];
    while context.tokens.peek() != Tok::RBrace {
        members.push({
            let attributes = parse_attributes(context)?;
            match context.tokens.peek() {
                // Top-level specification constructs
                Tok::Invariant => {
                    context.tokens.match_doc_comments();
                    ModuleMember::Spec(singleton_module_spec_block(
                        context,
                        context.tokens.start_loc(),
                        attributes,
                        parse_invariant,
                    )?)
                }
                Tok::Spec => {
                    match context.tokens.lookahead() {
                        Ok(Tok::Fun) | Ok(Tok::Native) => {
                            context.tokens.match_doc_comments();
                            let start_loc = context.tokens.start_loc();
                            context.tokens.advance()?;
                            // Add an extra check for better error message
                            // if old syntax is used
                            if context.tokens.lookahead2() == Ok((Tok::Identifier, Tok::LBrace)) {
                                return Err(unexpected_token_error(
                                    context.tokens,
                                    "only 'spec', drop the 'fun' keyword",
                                ));
                            }
                            ModuleMember::Spec(singleton_module_spec_block(
                                context,
                                start_loc,
                                attributes,
                                parse_spec_function,
                            )?)
                        }
                        _ => {
                            // Regular spec block
                            ModuleMember::Spec(parse_spec_block(attributes, context)?)
                        }
                    }
                }
                // Regular move constructs
                Tok::Friend => ModuleMember::Friend(parse_friend_decl(attributes, context)?),
                _ => {
                    context.tokens.match_doc_comments();
                    let start_loc = context.tokens.start_loc();
                    let modifiers = parse_module_member_modifiers(context)?;
                    match context.tokens.peek() {
                        Tok::Const => ModuleMember::Constant(parse_constant_decl(
                            attributes, start_loc, modifiers, context,
                        )?),
                        Tok::Fun => ModuleMember::Function(parse_function_decl(
                            attributes, start_loc, modifiers, context,
                        )?),
                        Tok::Struct => ModuleMember::Struct(parse_struct_decl(
                            attributes, start_loc, modifiers, context,
                        )?),
                        Tok::Enum => ModuleMember::Enum(parse_enum_decl(
                            attributes, start_loc, modifiers, context,
                        )?),
                        Tok::Use => ModuleMember::Use(parse_use_decl(
                            attributes, start_loc, modifiers, context,
                        )?),
                        _ => {
                            if matches!(context.tokens.peek(), Tok::Identifier)
                                && context.tokens.content() == "enum"
                                && !context.env.supports_feature(context.package_name, FeatureGate::Enums) {
                                if let Some(msg) = context.env.feature_edition_error_msg(FeatureGate::Enums, context.package_name) {
                                    let mut diag = diag!(Syntax::UnexpectedToken, (context.tokens.current_token_loc(), msg));
                                    diag.add_note(UPGRADE_NOTE);
                                    return Err(Box::new(diag));
                                } else {
                                    panic!("ICE edition checking should have produced an error")
                                }
                            } else {
                                let exp_string = if context.env.supports_feature(context.package_name, FeatureGate::Enums)
                                {
                                    format!(
                                        "a module member: '{}', '{}', '{}', '{}', '{}', '{}', or '{}'",
                                        Tok::Spec,
                                        Tok::Use,
                                        Tok::Friend,
                                        Tok::Const,
                                        Tok::Fun,
                                        Tok::Struct,
                                        Tok::Enum,
                                    )
                                } else {
                                    format!(
                                        "a module member: '{}', '{}', '{}', '{}', '{}', or '{}'",
                                        Tok::Spec,
                                        Tok::Use,
                                        Tok::Friend,
                                        Tok::Const,
                                        Tok::Fun,
                                        Tok::Struct,
                                    )
                                };
                                return Err(unexpected_token_error(context.tokens, &exp_string));
                            }
                        }
                    }
                }
            }
        })
    }
    consume_token(context.tokens, Tok::RBrace)?;
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    let def = ModuleDefinition {
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members,
    };

    Ok(def)
}

//**************************************************************************************************
// Specification Blocks
//**************************************************************************************************

// Parse an optional specification block:
//     SpecBlockTarget =
//          <Identifier>
//        |  "fun" <Identifier>  # deprecated
//        | "struct <Identifier> # deprecated
//        | "module"
//        | "schema" <Identifier> <OptionalTypeParameters>
//        | <empty>
//     SpecBlock =
//        <DocComments> "spec" ( <SpecFunction> | <SpecBlockTarget> "{" SpecBlockMember* "}" )
fn parse_spec_block(
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<SpecBlock, Box<Diagnostic>> {
    context.tokens.match_doc_comments();
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, Tok::Spec)?;
    let target_start_loc = context.tokens.start_loc();
    let target_ = match context.tokens.peek() {
        Tok::Fun => {
            return Err(unexpected_token_error(
                context.tokens,
                "only 'spec', drop the 'fun' keyword",
            ));
        }
        Tok::Struct => {
            return Err(unexpected_token_error(
                context.tokens,
                "only 'spec', drop the 'struct' keyword",
            ));
        }
        Tok::Module => {
            context.tokens.advance()?;
            SpecBlockTarget_::Module
        }
        Tok::Identifier if context.tokens.content() == "schema" => {
            context.tokens.advance()?;
            let name = parse_identifier(context)?;
            let type_parameters = parse_optional_type_parameters(context)?;
            SpecBlockTarget_::Schema(name, type_parameters)
        }
        Tok::RestrictedIdentifier | Tok::Identifier => {
            let name = parse_identifier(context)?;
            let signature = parse_spec_target_signature_opt(&name.loc, context)?;
            SpecBlockTarget_::Member(name, signature)
        }
        Tok::LBrace => SpecBlockTarget_::Code,
        _ => {
            return Err(unexpected_token_error(
                context.tokens,
                "one of `module`, `struct`, `fun`, `schema`, or `{`",
            ));
        }
    };
    let target = spanned(
        context.tokens.file_hash(),
        target_start_loc,
        match target_ {
            SpecBlockTarget_::Code => target_start_loc,
            _ => context.tokens.previous_end_loc(),
        },
        target_,
    );

    consume_token(context.tokens, Tok::LBrace)?;
    let mut uses = vec![];
    // TODO better errrors for modifiers or attributes
    while context.tokens.peek() == Tok::Use {
        let start_loc = context.tokens.start_loc();
        uses.push(parse_use_decl(
            vec![],
            start_loc,
            Modifiers::empty(),
            context,
        )?);
    }
    let mut members = vec![];
    while context.tokens.peek() != Tok::RBrace {
        members.push(parse_spec_block_member(context)?);
    }
    consume_token(context.tokens, Tok::RBrace)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlock_ {
            attributes,
            target,
            uses,
            members,
        },
    ))
}

fn parse_spec_target_signature_opt(
    loc: &Loc,
    context: &mut Context,
) -> Result<Option<Box<FunctionSignature>>, Box<Diagnostic>> {
    match context.tokens.peek() {
        Tok::Less | Tok::LParen => {
            let type_parameters = parse_optional_type_parameters(context)?;
            // "(" Comma<Parameter> ")"
            let parameters = parse_comma_list(
                context,
                Tok::LParen,
                Tok::RParen,
                parse_parameter,
                "a function parameter",
            )?;
            // (":" <Type>)?
            let return_type = if match_token(context.tokens, Tok::Colon)? {
                parse_type(context)?
            } else {
                sp(*loc, Type_::Unit)
            };
            Ok(Some(Box::new(FunctionSignature {
                type_parameters,
                parameters,
                return_type,
            })))
        }
        _ => Ok(None),
    }
}

// Parse a spec block member:
//    SpecBlockMember = <DocComments> ( <Invariant> | <Condition> | <SpecFunction> | <SpecVariable>
//                                   | <SpecInclude> | <SpecApply> | <SpecPragma> | <SpecLet>
//                                   | <SpecUpdate> | <SpecAxiom> )
fn parse_spec_block_member(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    context.tokens.match_doc_comments();
    match context.tokens.peek() {
        Tok::Invariant => parse_invariant(context),
        Tok::Let => parse_spec_let(context),
        Tok::Fun | Tok::Native => parse_spec_function(context),
        Tok::Identifier => match context.tokens.content() {
            "assert" | "assume" | "decreases" | "aborts_if" | "aborts_with" | "succeeds_if"
            | "modifies" | "emits" | "ensures" | "requires" => parse_condition(context),
            "axiom" => parse_axiom(context),
            "include" => parse_spec_include(context),
            "apply" => parse_spec_apply(context),
            "pragma" => parse_spec_pragma(context),
            "global" | "local" => parse_spec_variable(context),
            "update" => parse_spec_update(context),
            _ => {
                // local is optional but supported to be able to declare variables which are
                // named like the weak keywords above
                parse_spec_variable(context)
            }
        },
        _ => Err(unexpected_token_error(
            context.tokens,
            "one of `assert`, `assume`, `decreases`, `aborts_if`, `aborts_with`, `succeeds_if`, \
             `modifies`, `emits`, `ensures`, `requires`, `include`, `apply`, `pragma`, `global`, \
             or a name",
        )),
    }
}

// Parse a specification condition:
//    SpecCondition =
//        ("assert" | "assume" | "ensures" | "requires" ) <ConditionProperties> <Exp> ";"
//      | "aborts_if" <ConditionProperties> <Exp> ["with" <Exp>] ";"
//      | "aborts_with" <ConditionProperties> <Exp> [Comma <Exp>]* ";"
//      | "decreases" <ConditionProperties> <Exp> ";"
//      | "emits" <ConditionProperties> <Exp> "to" <Exp> [If <Exp>] ";"
fn parse_condition(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let kind_ = match context.tokens.content() {
        "assert" => SpecConditionKind_::Assert,
        "assume" => SpecConditionKind_::Assume,
        "decreases" => SpecConditionKind_::Decreases,
        "aborts_if" => SpecConditionKind_::AbortsIf,
        "aborts_with" => SpecConditionKind_::AbortsWith,
        "succeeds_if" => SpecConditionKind_::SucceedsIf,
        "modifies" => SpecConditionKind_::Modifies,
        "emits" => SpecConditionKind_::Emits,
        "ensures" => SpecConditionKind_::Ensures,
        "requires" => SpecConditionKind_::Requires,
        _ => unreachable!(),
    };
    context.tokens.advance()?;
    let kind = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        kind_.clone(),
    );
    let properties = parse_condition_properties(context)?;
    let exp = if kind_ == SpecConditionKind_::AbortsWith || kind_ == SpecConditionKind_::Modifies {
        // Use a dummy expression as a placeholder for this field.
        let loc = make_loc(context.tokens.file_hash(), start_loc, start_loc + 1);
        sp(loc, Exp_::Value(sp(loc, Value_::Bool(false))))
    } else {
        parse_exp(context)?
    };
    let additional_exps = if kind_ == SpecConditionKind_::AbortsIf
        && context.tokens.peek() == Tok::Identifier
        && context.tokens.content() == "with"
    {
        context.tokens.advance()?;
        let codes = vec![parse_exp(context)?];
        consume_token(context.tokens, Tok::Semicolon)?;
        codes
    } else if kind_ == SpecConditionKind_::AbortsWith || kind_ == SpecConditionKind_::Modifies {
        parse_comma_list_after_start(
            context,
            context.tokens.start_loc(),
            context.tokens.peek(),
            Tok::Semicolon,
            parse_exp,
            "an aborts code or modifies target",
        )?
    } else if kind_ == SpecConditionKind_::Emits {
        consume_identifier(context.tokens, "to")?;
        let mut additional_exps = vec![parse_exp(context)?];
        if match_token(context.tokens, Tok::If)? {
            additional_exps.push(parse_exp(context)?);
        }
        consume_token(context.tokens, Tok::Semicolon)?;
        additional_exps
    } else {
        consume_token(context.tokens, Tok::Semicolon)?;
        vec![]
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        SpecBlockMember_::Condition {
            kind,
            properties,
            exp,
            additional_exps,
        },
    ))
}

// Parse properties in a condition.
//   ConditionProperties = ( "[" Comma<SpecPragmaProperty> "]" )?
fn parse_condition_properties(
    context: &mut Context,
) -> Result<Vec<PragmaProperty>, Box<Diagnostic>> {
    let properties = if context.tokens.peek() == Tok::LBracket {
        parse_comma_list(
            context,
            Tok::LBracket,
            Tok::RBracket,
            parse_spec_property,
            "a condition property",
        )?
    } else {
        vec![]
    };
    Ok(properties)
}

// Parse an axiom:
//     a = "axiom" <OptionalTypeParameters> <ConditionProperties> <Exp> ";"
fn parse_axiom(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_identifier(context.tokens, "axiom")?;
    let type_parameters = parse_optional_type_parameters(context)?;
    let kind = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecConditionKind_::Axiom(type_parameters),
    );
    let properties = parse_condition_properties(context)?;
    let exp = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Condition {
            kind,
            properties,
            exp,
            additional_exps: vec![],
        },
    ))
}

// Parse an invariant:
//     Invariant = "invariant" <OptionalTypeParameters> [ "update" ] <ConditionProperties> <Exp> ";"
fn parse_invariant(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, Tok::Invariant)?;
    let type_parameters = parse_optional_type_parameters(context)?;
    let kind_ = match context.tokens.peek() {
        Tok::Identifier if context.tokens.content() == "update" => {
            context.tokens.advance()?;
            SpecConditionKind_::InvariantUpdate(type_parameters)
        }
        _ => SpecConditionKind_::Invariant(type_parameters),
    };
    let kind = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        kind_,
    );
    let properties = parse_condition_properties(context)?;
    let exp = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Condition {
            kind,
            properties,
            exp,
            additional_exps: vec![],
        },
    ))
}

// Parse a specification function.
//     SpecFunction = "define" <SpecFunctionSignature> ( "{" <Sequence> "}" | ";" )
//                  | "native" "define" <SpecFunctionSignature> ";"
//     SpecFunctionSignature =
//         <Identifier> <OptionalTypeParameters> "(" Comma<Parameter> ")" ":" <Type>
fn parse_spec_function(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let native_opt = consume_optional_token_with_loc(context.tokens, Tok::Native)?;
    consume_token(context.tokens, Tok::Fun)?;
    let name = FunctionName(parse_identifier(context)?);
    let type_parameters = parse_optional_type_parameters(context)?;
    // "(" Comma<Parameter> ")"
    let parameters = parse_comma_list(
        context,
        Tok::LParen,
        Tok::RParen,
        parse_parameter,
        "a function parameter",
    )?;

    // ":" <Type>)
    consume_token(context.tokens, Tok::Colon)?;
    let return_type = parse_type(context)?;

    let body_start_loc = context.tokens.start_loc();
    let no_body = context.tokens.peek() != Tok::LBrace;
    let (uninterpreted, body_) = if native_opt.is_some() || no_body {
        consume_token(context.tokens, Tok::Semicolon)?;
        (native_opt.is_none(), FunctionBody_::Native)
    } else {
        consume_token(context.tokens, Tok::LBrace)?;
        let seq = parse_sequence(context)?;
        (false, FunctionBody_::Defined(seq))
    };
    let body = spanned(
        context.tokens.file_hash(),
        body_start_loc,
        context.tokens.previous_end_loc(),
        body_,
    );

    let signature = FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    };

    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Function {
            signature,
            uninterpreted,
            name,
            body,
        },
    ))
}

// Parse a specification variable.
//     SpecVariable = ( "global" | "local" )?
//                    <Identifier> <OptionalTypeParameters>
//                    ":" <Type>
//                    [ "=" Exp ]  // global only
//                    ";"
fn parse_spec_variable(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let is_global = match context.tokens.content() {
        "global" => {
            consume_token(context.tokens, Tok::Identifier)?;
            true
        }
        "local" => {
            consume_token(context.tokens, Tok::Identifier)?;
            false
        }
        _ => false,
    };
    let name = parse_identifier(context)?;
    let type_parameters = parse_optional_type_parameters(context)?;
    consume_token(context.tokens, Tok::Colon)?;
    let type_ = parse_type(context)?;
    let init = if is_global && context.tokens.peek() == Tok::Equal {
        context.tokens.advance()?;
        Some(parse_exp(context)?)
    } else {
        None
    };

    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Variable {
            is_global,
            name,
            type_parameters,
            type_,
            init,
        },
    ))
}

// Parse a specification update.
//     SpecUpdate = "update" <Exp> = <Exp> ";"
fn parse_spec_update(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_token(context.tokens, Tok::Identifier)?;
    let lhs = parse_unary_exp(context)?;
    consume_token(context.tokens, Tok::Equal)?;
    let rhs = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Update { lhs, rhs },
    ))
}

// Parse a specification let.
//     SpecLet =  "let" [ "post" ] <Identifier> "=" <Exp> ";"
fn parse_spec_let(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    context.tokens.advance()?;
    let post_state =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "post" {
            context.tokens.advance()?;
            true
        } else {
            false
        };
    let name = parse_identifier(context)?;
    consume_token(context.tokens, Tok::Equal)?;
    let def = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Let {
            name,
            post_state,
            def,
        },
    ))
}

// Parse a specification schema include.
//    SpecInclude = "include" <Exp>
fn parse_spec_include(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_identifier(context.tokens, "include")?;
    let properties = parse_condition_properties(context)?;
    let exp = parse_exp(context)?;
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Include { properties, exp },
    ))
}

// Parse a specification schema apply.
//    SpecApply = "apply" <Exp> "to" Comma<SpecApplyPattern>
//                                   ( "except" Comma<SpecApplyPattern> )? ";"
fn parse_spec_apply(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_identifier(context.tokens, "apply")?;
    let exp = parse_exp(context)?;
    consume_identifier(context.tokens, "to")?;
    let parse_patterns = |context: &mut Context| {
        parse_list(
            context,
            |context| {
                if context.tokens.peek() == Tok::Comma {
                    context.tokens.advance()?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
            parse_spec_apply_pattern,
        )
    };
    let patterns = parse_patterns(context)?;
    let exclusion_patterns =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "except" {
            context.tokens.advance()?;
            parse_patterns(context)?
        } else {
            vec![]
        };
    consume_token(context.tokens, Tok::Semicolon)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Apply {
            exp,
            patterns,
            exclusion_patterns,
        },
    ))
}

// Parse a function pattern:
//     SpecApplyPattern = <SpecApplyFragment>+ <OptionalTypeArgs>
fn parse_spec_apply_pattern(context: &mut Context) -> Result<SpecApplyPattern, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    // TODO: update the visibility parsing in the spec as well
    let public_opt = consume_optional_token_with_loc(context.tokens, Tok::Public)?;
    let visibility = if let Some(loc) = public_opt {
        Some(Visibility::Public(loc))
    } else if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "internal" {
        // Its not ideal right now that we do not have a loc here, but acceptable for what
        // we are doing with this in specs.
        context.tokens.advance()?;
        Some(Visibility::Internal)
    } else {
        None
    };
    let mut last_end = context.tokens.start_loc() + context.tokens.content().len();
    let name_pattern = parse_list(
        context,
        |context| {
            // We need name fragments followed by each other without space. So we do some
            // magic here similar as with `>>` based on token distance.
            let start_loc = context.tokens.start_loc();
            let adjacent = last_end == start_loc;
            last_end = start_loc + context.tokens.content().len();
            Ok(adjacent && [Tok::Identifier, Tok::Star].contains(&context.tokens.peek()))
        },
        parse_spec_apply_fragment,
    )?;
    let type_parameters = parse_optional_type_parameters(context)?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecApplyPattern_ {
            visibility,
            name_pattern,
            type_parameters,
        },
    ))
}

// Parse a name pattern fragment
//     SpecApplyFragment = <Identifier> | "*"
fn parse_spec_apply_fragment(context: &mut Context) -> Result<SpecApplyFragment, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let fragment = match context.tokens.peek() {
        Tok::Identifier => SpecApplyFragment_::NamePart(parse_identifier(context)?),
        Tok::Star => {
            context.tokens.advance()?;
            SpecApplyFragment_::Wildcard
        }
        _ => {
            return Err(unexpected_token_error(
                context.tokens,
                "a name fragment or `*`",
            ))
        }
    };
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        fragment,
    ))
}

// Parse a specification pragma:
//    SpecPragma = "pragma" Comma<SpecPragmaProperty> ";"
fn parse_spec_pragma(context: &mut Context) -> Result<SpecBlockMember, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    consume_identifier(context.tokens, "pragma")?;
    let properties = parse_comma_list_after_start(
        context,
        start_loc,
        Tok::Identifier,
        Tok::Semicolon,
        parse_spec_property,
        "a pragma property",
    )?;
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        SpecBlockMember_::Pragma { properties },
    ))
}

// Parse a specification pragma property:
//    SpecPragmaProperty = <Identifier> ( "=" <Value> | <NameAccessChain> )?
fn parse_spec_property(context: &mut Context) -> Result<PragmaProperty, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let name = match consume_optional_token_with_loc(context.tokens, Tok::Friend)? {
        // special treatment for `pragma friend = ...` as friend is a keyword
        // TODO: this might violate the assumption that a keyword can never be a name.
        Some(loc) => Name::new(loc, symbol!("friend")),
        None => parse_identifier(context)?,
    };
    let value = if context.tokens.peek() == Tok::Equal {
        context.tokens.advance()?;
        match context.tokens.peek() {
            Tok::AtSign | Tok::True | Tok::False | Tok::NumTypedValue | Tok::ByteStringValue => {
                Some(PragmaValue::Literal(parse_value(context)?))
            }
            Tok::NumValue
                if !context
                    .tokens
                    .lookahead()
                    .map(|tok| tok == Tok::ColonColon)
                    .unwrap_or(false) =>
            {
                Some(PragmaValue::Literal(parse_value(context)?))
            }
            _ => {
                // Parse as a module access for a possibly qualified identifier
                Some(PragmaValue::Ident(parse_name_access_chain(
                    context,
                    || "an identifier as pragma value",
                )?))
            }
        }
    } else {
        None
    };
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        PragmaProperty_ { name, value },
    ))
}

/// Creates a module spec block for a single member.
fn singleton_module_spec_block(
    context: &mut Context,
    start_loc: usize,
    attributes: Vec<Attributes>,
    member_parser: impl Fn(&mut Context) -> Result<SpecBlockMember, Box<Diagnostic>>,
) -> Result<SpecBlock, Box<Diagnostic>> {
    let member = member_parser(context)?;
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        SpecBlock_ {
            attributes,
            target: spanned(
                context.tokens.file_hash(),
                start_loc,
                start_loc,
                SpecBlockTarget_::Module,
            ),
            uses: vec![],
            members: vec![member],
        },
    ))
}

//**************************************************************************************************
// File
//**************************************************************************************************

// Parse a file:
//      File =
//          (<Attributes> (<AddressBlock> | <Module> ))*
fn parse_file(context: &mut Context) -> Result<Vec<Definition>, Box<Diagnostic>> {
    let mut defs = vec![];
    while context.tokens.peek() != Tok::EOF {
        let attributes = parse_attributes(context)?;
        defs.push(match context.tokens.peek() {
            Tok::Spec | Tok::Module => Definition::Module(parse_module(attributes, context)?),
            _ => Definition::Address(parse_address_block(attributes, context)?),
        })
    }
    Ok(defs)
}

/// Parse the `input` string as a file of Move source code and return the
/// result as either a pair of FileDefinition and doc comments or some Diagnostics. The `file` name
/// is used to identify source locations in error messages.
pub fn parse_file_string(
    env: &mut CompilationEnv,
    file_hash: FileHash,
    input: &str,
    package: Option<Symbol>,
) -> Result<(Vec<Definition>, MatchedFileCommentMap), Diagnostics> {
    let edition = env.edition(package);
    let mut tokens = Lexer::new(input, file_hash, edition);
    match tokens.advance() {
        Err(err) => Err(Diagnostics::from(vec![*err])),
        Ok(..) => Ok(()),
    }?;
    match parse_file(&mut Context::new(env, &mut tokens, package)) {
        Err(err) => Err(Diagnostics::from(vec![*err])),
        Ok(def) => Ok((def, tokens.check_and_get_doc_comments(env))),
    }
}
