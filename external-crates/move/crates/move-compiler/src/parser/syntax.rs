// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// In the informal grammar comments in this file, Comma<T> is shorthand for:
//      (<T> ",")* <T>?
// Note that this allows an optional trailing comma.

use crate::{
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter, Diagnostics},
    editions::{Edition, FeatureGate, UPGRADE_NOTE},
    parser::{ast::*, lexer::*, token_set::*},
    shared::{string_utils::*, *},
};

use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::{symbol, Symbol};

struct Context<'env, 'lexer, 'input> {
    current_package: Option<Symbol>,
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    tokens: &'lexer mut Lexer<'input>,
    stop_set: TokenSet,
}

impl<'env, 'lexer, 'input> Context<'env, 'lexer, 'input> {
    fn new(
        env: &'env CompilationEnv,
        tokens: &'lexer mut Lexer<'input>,
        package_name: Option<Symbol>,
    ) -> Self {
        let stop_set = TokenSet::from([Tok::EOF]);
        let reporter = env.diagnostic_reporter_at_top_level();
        Self {
            current_package: package_name,
            env,
            reporter,
            tokens,
            stop_set,
        }
    }

    /// Checks if the current token is a member of the stop set.
    fn at_stop_set(&self) -> bool {
        self.tokens.at_set(&self.stop_set)
    }

    /// Advances tokens until reaching an element of the stop set, recording diagnostics along the
    /// way (including the first optional one passed as an argument).
    fn advance_until_stop_set(&mut self, diag_opt: Option<Diagnostic>) {
        if let Some(diag) = diag_opt {
            self.add_diag(diag);
        }
        while !self.at_stop_set() {
            if let Err(err) = self.tokens.advance() {
                self.add_diag(*err);
            }
        }
    }

    fn at_end(&self, prev: Loc) -> bool {
        prev.end() as usize == self.tokens.start_loc()
    }

    /// Advances token and records a resulting diagnostic (if any).
    fn advance(&mut self) {
        if let Err(diag) = self.tokens.advance() {
            self.add_diag(*diag);
        }
    }

    fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    fn check_feature(&self, package: Option<Symbol>, feature: FeatureGate, loc: Loc) -> bool {
        self.env
            .check_feature(&self.reporter, package, feature, loc)
    }
}

//**************************************************************************************************
// Error Handling
//**************************************************************************************************

const EOF_ERROR_STR: &str = "end-of-file";

fn current_token_error_string(tokens: &Lexer) -> String {
    if tokens.peek() == Tok::EOF {
        EOF_ERROR_STR.to_string()
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

/// Error when parsing a module member with a special case when (unexpectedly) encountering another
/// module to be parsed.
enum ErrCase {
    Unknown(Box<Diagnostic>),
    ContinueToModule(Vec<Attributes>),
}

impl From<Box<Diagnostic>> for ErrCase {
    fn from(diag: Box<Diagnostic>) -> Self {
        ErrCase::Unknown(diag)
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
    item_first_set: &TokenSet,
    parse_list_item: F,
    item_description: &str,
) -> Vec<R>
where
    F: Fn(&mut Context) -> Result<R, Box<Diagnostic>>,
{
    let start_loc = context.tokens.start_loc();
    let at_start_token = context.tokens.at(start_token);
    if let Err(diag) = consume_token(context.tokens, start_token) {
        if !at_start_token {
            // not even starting token is present - parser has nothing much to do
            context.add_diag(*diag);
            return vec![];
        }
        // advance token past the starting one but something went wrong, still there is a chance
        // parse the rest of the list
        advance_separated_items_error(
            context,
            start_token,
            end_token,
            /* separator */ Tok::Comma,
            /* for list */ true,
            *diag,
        );
        if context.at_stop_set() {
            // nothing else to do
            return vec![];
        }
        if context.tokens.at(end_token) {
            // at the end of the list - consume end token and keep parsing at the outer level
            context.advance();
            return vec![];
        }
    }
    parse_comma_list_after_start(
        context,
        start_loc,
        start_token,
        end_token,
        item_first_set,
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
    item_first_set: &TokenSet,
    parse_list_item: F,
    item_description: &str,
) -> Vec<R>
where
    F: Fn(&mut Context) -> Result<R, Box<Diagnostic>>,
{
    adjust_token(context.tokens, end_token);
    let mut v = vec![];
    while !context.tokens.at(end_token) {
        if context.tokens.at_set(item_first_set) {
            match parse_list_item(context) {
                Ok(item) => {
                    v.push(item);
                    adjust_token(context.tokens, end_token);
                    if context.tokens.peek() == end_token || context.at_stop_set() {
                        break;
                    }
                    // expect a commma - since we are not at stop set, consume it or advance to the
                    // next time or end of the list
                    if context.tokens.at(Tok::Comma) {
                        context.advance();
                    } else {
                        let diag = unexpected_token_error(
                            context.tokens,
                            &format_oxford_list!("or", "'{}'", &[Tok::Comma, end_token]),
                        );
                        advance_separated_items_error(
                            context,
                            start_token,
                            end_token,
                            /* separator */ Tok::Comma,
                            /* for list */ true,
                            *diag,
                        );
                        if context.at_stop_set() {
                            break;
                        }
                    }
                    adjust_token(context.tokens, end_token);
                    // everything worked out so simply continue
                    continue;
                }
                Err(diag) => {
                    advance_separated_items_error(
                        context,
                        start_token,
                        end_token,
                        /* separator */ Tok::Comma,
                        /* for list */ true,
                        *diag,
                    );
                }
            }
        } else {
            let current_loc = context.tokens.start_loc();
            let loc = make_loc(context.tokens.file_hash(), current_loc, current_loc);
            let diag = diag!(
                Syntax::UnexpectedToken,
                (
                    loc,
                    format!(
                        "Unexpected '{}'. Expected {}",
                        context.tokens.peek(),
                        item_description
                    )
                )
            );
            advance_separated_items_error(
                context,
                start_token,
                end_token,
                /* separator */ Tok::Comma,
                /* for list */ true,
                diag,
            );
        }
        // The stop set check is done at the end of the loop on purpose as we need to attempt
        // parsing before checking it. If we do not, in the best case, we will get a less meaningful
        // error message if the item belongs to the token set incorrectly (e.g., `fun` keyword), and
        // in the worst case, we will get an error in the correct code (e.g., if a function argument
        // is named `entry`)
        if context.at_stop_set() {
            break;
        }
    }
    if consume_token(context.tokens, end_token).is_err() {
        let current_loc = context.tokens.start_loc();
        let loc = make_loc(context.tokens.file_hash(), current_loc, current_loc);
        let loc2 = make_loc(context.tokens.file_hash(), start_loc, start_loc);
        context.add_diag(diag!(
            Syntax::UnexpectedToken,
            (loc, format!("Expected '{}'", end_token)),
            (loc2, format!("To match this '{}'", start_token)),
        ));
    }
    v
}

/// Attempts to skip tokens until the end of the item in a series of separated (which started with
/// an already consumed starting token) - looks for a matched ending token or a token appearing
/// after the separator. This helper function is used when parsing lists and sequences.
fn advance_separated_items_error(
    context: &mut Context,
    start_token: Tok,
    end_token: Tok,
    sep_token: Tok,
    for_list: bool,
    diag: Diagnostic,
) {
    context.add_diag(diag);
    let mut depth: i32 = 0; // When we find  another start token, we track how deep we are in them
    loop {
        // adjusting tokens (replacing `<<` with `<`) makes sense only when parsing lists and it
        // would feel odd to also do this when using this helper function to parse other things
        // (e.g., sequences)
        if for_list {
            adjust_token(context.tokens, end_token);
        }
        if context.at_stop_set() {
            break;
        }
        if depth == 0 {
            if context.tokens.at(end_token) {
                break;
            }
            if context.tokens.at(sep_token) {
                context.advance();
                break;
            }
        }

        if context.tokens.at(sep_token) {
            assert!(depth > 0);
        } else if context.tokens.at(start_token) {
            depth += 1;
        } else if context.tokens.at(end_token) {
            assert!(depth > 0);
            depth -= 1;
        }
        context.advance();
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

fn match_doc_comments(context: &mut Context) -> DocComment {
    let comment_opt = context
        .tokens
        .take_doc_comment()
        .map(|(start, end, comment)| {
            let loc = Loc::new(context.tokens.file_hash(), start, end);
            sp(loc, comment)
        });
    DocComment(comment_opt)
}

fn check_no_doc_comment(context: &mut Context, loc: Loc, case: &str, doc: DocComment) {
    if let Some(doc_loc) = doc.loc() {
        let doc_msg = "Unexpected documentation comment";
        let msg = format!("Documentation comments are not supported on {case}");
        context.add_diag(diag!(
            Syntax::InvalidDocComment,
            (doc_loc, doc_msg),
            (loc, msg),
        ));
    }
}

//**************************************************************************************************
// Identifiers, Addresses, and Names
//**************************************************************************************************

fn report_name_migration(context: &mut Context, name: &str, loc: Loc) {
    context.add_diag(diag!(Migration::NeedsRestrictedIdentifier, (loc, name)));
}

// Parse an identifier:
//      Identifier = <IdentifierValue>
//
// Expects the current token to be Tok::Identifier or Tok::RestrictedIdentifier and returns
// `Syntax::UnexpectedToken` for the current token if it is not.
fn parse_identifier(context: &mut Context) -> Result<Name, Box<Diagnostic>> {
    let id: Symbol = match context.tokens.peek() {
        Tok::Identifier => context.tokens.content().into(),
        Tok::RestrictedIdentifier => {
            // peel off backticks ``
            let content = context.tokens.content();
            let peeled = &content[1..content.len() - 1];
            peeled.into()
        }
        // carve-out for migration with new keywords
        tok @ (Tok::Mut | Tok::Match | Tok::For | Tok::Enum | Tok::Type)
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
        {
            report_name_migration(
                context,
                &format!("{}", tok),
                context.tokens.current_token_loc(),
            );
            context.tokens.content().into()
        }
        _ => {
            return Err(unexpected_token_error(context.tokens, "an identifier"));
        }
    };
    let start_loc = context.tokens.start_loc();
    context.advance();
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, id))
}

// Parse a macro parameter identifier.
// The name, SyntaxIdentifier, comes from the usage of the identifier to perform expression
// substitution in a macro invocation, i.e. a syntactic substitution.
//      SyntaxIdentifier = <SyntaxIdentifierValue>
fn parse_syntax_identifier(context: &mut Context) -> Result<Name, Box<Diagnostic>> {
    if context.tokens.peek() != Tok::SyntaxIdentifier {
        return Err(unexpected_token_error(
            context.tokens,
            "an identifier prefixed by '$'",
        ));
    }
    let start_loc = context.tokens.start_loc();
    let id = context.tokens.content().into();
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
            context.add_diag(diag!(Syntax::InvalidAddress, (loc, msg)));
            NumericalAddress::DEFAULT_ERROR_ADDRESS
        }
    };
    Ok(sp(loc, addr_))
}

// Parse the beginning of an access, either an address or an identifier:
//      LeadingNameAccess = <NumericalAddress> | <Identifier> | <SyntaxIdentifier>
fn parse_leading_name_access(context: &mut Context) -> Result<LeadingNameAccess, Box<Diagnostic>> {
    parse_leading_name_access_(context, false, &|| "an address or an identifier")
}

// Parse the beginning of an access, either an address or an identifier with a specific description
fn parse_leading_name_access_<'a, F: Fn() -> &'a str>(
    context: &mut Context,
    global_name: bool,
    item_description: &F,
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
        Tok::SyntaxIdentifier => {
            let loc = current_token_loc(context.tokens);
            let n = parse_syntax_identifier(context)?;
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
        // carve-out for migration with new keywords
        Tok::Mut | Tok::Match | Tok::For | Tok::Enum | Tok::Type
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
        {
            if global_name {
                Err(unexpected_token_error(context.tokens, item_description()))
            } else {
                let loc = current_token_loc(context.tokens);
                let n = parse_identifier(context)?;
                let name = LeadingNameAccess_::Name(n);
                Ok(sp(loc, name))
            }
        }
        _ => Err(unexpected_token_error(context.tokens, item_description())),
    }
}

// Parse a variable name:
//      Var = <Identifier> | <SyntaxIdentifier>
fn parse_var(context: &mut Context) -> Result<Var, Box<Diagnostic>> {
    Ok(Var(match context.tokens.peek() {
        Tok::SyntaxIdentifier => parse_syntax_identifier(context)?,
        _ => parse_identifier(context)?,
    }))
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

// Parse a module access (a variable, struct type, or function):
//      NameAccessChain =
//          <LeadingNameAccess> <OptionalTypeArgs>
//              ( "::" <Identifier> <OptionalTypeArgs> )^n
//
//  (n in {0,1,2,3})
//  Returns a name and an indicator if we parsed a macro. Note that if `n > 3`, this parses those
//  and reports errors.
fn parse_name_access_chain<'a, F: Fn() -> &'a str>(
    context: &mut Context,
    macros_allowed: bool,
    tyargs_allowed: bool,
    item_description: F,
) -> Result<NameAccessChain, Box<Diagnostic>> {
    ok_with_loc!(context, {
        parse_name_access_chain_(
            context,
            macros_allowed,
            tyargs_allowed,
            false,
            item_description,
        )?
    })
}

// Parse a module access allowing whitespace for type arguments.
// Implicitly allows parsing of type arguments.
fn parse_name_access_chain_with_tyarg_whitespace<'a, F: Fn() -> &'a str>(
    context: &mut Context,
    macros_allowed: bool,
    item_description: F,
) -> Result<NameAccessChain, Box<Diagnostic>> {
    ok_with_loc!(context, {
        parse_name_access_chain_(context, macros_allowed, true, true, item_description)?
    })
}

// Parse a module access with a specific description
fn parse_name_access_chain_<'a, F: Fn() -> &'a str>(
    context: &mut Context,
    macros_allowed: bool,
    tyargs_allowed: bool,
    tyargs_whitespace_allowed: bool,
    item_description: F,
) -> Result<NameAccessChain_, Box<Diagnostic>> {
    use LeadingNameAccess_ as LN;

    let global_name = if context.tokens.peek() == Tok::ColonColon {
        context.tokens.advance()?;
        true
    } else {
        false
    };

    let ln = parse_leading_name_access_(context, global_name, &item_description)?;

    let (mut is_macro, mut tys) =
        parse_macro_opt_and_tyargs_opt(context, tyargs_whitespace_allowed, ln.loc);
    if let Some(loc) = &is_macro {
        if !macros_allowed {
            let msg = format!(
                "Macro invocation are disallowed here. Expected {}",
                item_description()
            );
            context.add_diag(diag!(Syntax::InvalidName, (*loc, msg)));
            is_macro = None;
        }
    }
    if let Some(sp!(ty_loc, _)) = tys {
        if !tyargs_allowed {
            context.add_diag(diag!(
                Syntax::InvalidName,
                (
                    ty_loc,
                    format!(
                        "Type arguments are disallowed here. Expected {}",
                        item_description()
                    )
                )
            ));
            tys = None;
        }
    }

    let ln = match ln {
        // A name by itself is a valid access chain
        sp!(_, LN::Name(n1)) if context.tokens.peek() != Tok::ColonColon => {
            let single = PathEntry {
                name: n1,
                tyargs: tys,
                is_macro,
            };
            return Ok(NameAccessChain_::Single(single));
        }
        ln => ln,
    };

    if matches!(ln.value, LN::GlobalAddress(_) | LN::AnonymousAddress(_))
        && context.tokens.peek() != Tok::ColonColon
    {
        let addr_msg = match &ln.value {
            LN::AnonymousAddress(_) => "anonymous",
            LN::GlobalAddress(_) => "global",
            LN::Name(_) => "named",
        };
        let mut diag = diag!(
            Syntax::UnexpectedToken,
            (
                ln.loc,
                format!(
                    "Expected '::' after the {} address in this module access chain",
                    addr_msg
                )
            )
        );
        if matches!(ln.value, LN::GlobalAddress(_)) {
            diag.add_note("Access chains that start with '::' must be multi-part");
        }
        return Err(Box::new(diag));
    }

    let root = RootPathEntry {
        name: ln,
        tyargs: tys,
        is_macro,
    };

    let mut path = NameAccessChain_::path(root);
    while context.tokens.peek() == Tok::ColonColon {
        consume_token_(
            context.tokens,
            Tok::ColonColon,
            context.tokens.start_loc(),
            " after an address in a module access chain",
        )?;
        let name = match parse_identifier(context) {
            Ok(ident) => ident,
            Err(_) => {
                // diagnostic for this is reported in path expansion (as a parsing error) when we
                // detect incomplete chaing (adding "default" diag here would make error reporting
                // somewhat redundant in this case)
                path.is_incomplete = true;
                return Ok(NameAccessChain_::Path(path));
            }
        };
        let (mut is_macro, mut tys) =
            parse_macro_opt_and_tyargs_opt(context, tyargs_whitespace_allowed, name.loc);
        if let Some(loc) = &is_macro {
            if !macros_allowed {
                context.add_diag(diag!(
                    Syntax::InvalidName,
                    (
                        *loc,
                        format!("Cannot use macro invocation '!' in {}", item_description())
                    )
                ));
                is_macro = None;
            }
        }
        if let Some(sp!(ty_loc, _)) = tys {
            if !tyargs_allowed {
                context.add_diag(diag!(
                    Syntax::InvalidName,
                    (
                        ty_loc,
                        format!("Cannot use type arguments in {}", item_description())
                    )
                ));
                tys = None;
            }
        }

        path.push_path_entry(name, tys, is_macro)
            .into_iter()
            .for_each(|diag| context.add_diag(diag));
    }
    Ok(NameAccessChain_::Path(path))
}

fn parse_macro_opt_and_tyargs_opt(
    context: &mut Context,
    tyargs_whitespace_allowed: bool,
    end_loc: Loc,
) -> (Option<Loc>, Option<Spanned<Vec<Type>>>) {
    let mut is_macro = None;
    let mut tyargs = None;

    if let Tok::Exclaim = context.tokens.peek() {
        let loc = current_token_loc(context.tokens);
        context.advance();
        is_macro = Some(loc);
    }

    // There's an ambiguity if the name is followed by a '<'.
    // If there is no whitespace after the name or if a macro call has been started,
    //   treat it as the start of a list of type arguments.
    // Otherwise, assume that the '<' is a boolean operator.
    let _start_loc = context.tokens.start_loc();
    if context.tokens.peek() == Tok::Less
        && (context.at_end(end_loc) || is_macro.is_some() || tyargs_whitespace_allowed)
    {
        let start_loc = context.tokens.start_loc();
        let tys_ = parse_optional_type_args(context);
        let ty_loc = make_loc(
            context.tokens.file_hash(),
            start_loc,
            context.tokens.previous_end_loc(),
        );
        tyargs = tys_.map(|tys| sp(ty_loc, tys));
    }
    (is_macro, tyargs)
}

//**************************************************************************************************
// Modifiers
//**************************************************************************************************

struct Modifiers {
    visibility: Option<Visibility>,
    entry: Option<Loc>,
    native: Option<Loc>,
    macro_: Option<Loc>,
}

impl Modifiers {
    fn empty() -> Self {
        Self {
            visibility: None,
            entry: None,
            native: None,
            macro_: None,
        }
    }
}

// Parse module member modifiers: visiblility and native.
//      ModuleMemberModifiers = <ModuleMemberModifier>*
//      ModuleMemberModifier = <Visibility> | "native"
// ModuleMemberModifiers checks for uniqueness, meaning each individual ModuleMemberModifier can
// appear only once
fn parse_module_member_modifiers(context: &mut Context) -> Result<Modifiers, Box<Diagnostic>> {
    fn duplicate_modifier_error(
        context: &mut Context,
        loc: Loc,
        prev_loc: Loc,
        modifier_name: &'static str,
    ) {
        let msg = format!("Duplicate '{modifier_name}' modifier");
        let prev_msg = format!("'{modifier_name}' modifier previously given here");
        context.add_diag(diag!(
            Declarations::DuplicateItem,
            (loc, msg),
            (prev_loc, prev_msg),
        ));
    }

    let mut mods = Modifiers::empty();
    loop {
        match context.tokens.peek() {
            Tok::Public => {
                let vis = parse_visibility(context)?;
                if let Some(prev_vis) = mods.visibility {
                    duplicate_modifier_error(
                        context,
                        vis.loc().unwrap(),
                        prev_vis.loc().unwrap(),
                        Visibility::PUBLIC,
                    )
                }
                mods.visibility = Some(vis)
            }
            Tok::Native => {
                let loc = current_token_loc(context.tokens);
                context.tokens.advance()?;
                if let Some(prev_loc) = mods.native {
                    duplicate_modifier_error(context, loc, prev_loc, NATIVE_MODIFIER)
                }
                mods.native = Some(loc)
            }
            Tok::Identifier if context.tokens.content() == ENTRY_MODIFIER => {
                let loc = current_token_loc(context.tokens);
                context.tokens.advance()?;
                if let Some(prev_loc) = mods.entry {
                    duplicate_modifier_error(context, loc, prev_loc, ENTRY_MODIFIER)
                }
                mods.entry = Some(loc)
            }
            Tok::Identifier if context.tokens.content() == MACRO_MODIFIER => {
                let loc = current_token_loc(context.tokens);
                context.tokens.advance()?;
                if let Some(prev_loc) = mods.macro_ {
                    duplicate_modifier_error(context, loc, prev_loc, MACRO_MODIFIER)
                }
                mods.macro_ = Some(loc)
            }
            _ => break,
        }
    }
    Ok(mods)
}

fn check_no_modifier(
    context: &mut Context,
    modifier_name: &'static str,
    modifier_loc: Option<Loc>,
    module_member: &str,
) {
    const LOCATIONS: &[(&str, &str)] = &[
        (NATIVE_MODIFIER, "functions or structs"),
        (ENTRY_MODIFIER, "functions"),
        (MACRO_MODIFIER, "functions"),
    ];
    let Some(loc) = modifier_loc else { return };
    let location = LOCATIONS
        .iter()
        .find(|(name, _)| *name == modifier_name)
        .unwrap()
        .1;
    let msg = format!(
        "Invalid {module_member} declaration. '{modifier_name}' is used only on {location}",
    );
    context.add_diag(diag!(Syntax::InvalidModifier, (loc, msg)));
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

    let ma = parse_name_access_chain(
        context,
        /* macros */ false,
        /* tyargs */ false,
        || "attribute name value",
    )?;
    Ok(sp(ma.loc, AttributeValue_::ModuleAccess(ma)))
}

// Parse a single attribute
//      Attribute =
//          "for"
//          | <Identifier>
//          | <Identifier> "=" <AttributeValue>
//          | <Identifier> "(" Comma<Attribute> ")"
fn parse_attribute(context: &mut Context) -> Result<Attribute, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let n = match context.tokens.peek() {
        // hack for `#[syntax(for)]` attribute
        Tok::For => {
            let for_ = context.tokens.content().into();
            context.tokens.advance()?;
            let end_loc = context.tokens.previous_end_loc();
            spanned(context.tokens.file_hash(), start_loc, end_loc, for_)
        }
        _ => parse_identifier(context)?,
    };
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
                &ATTR_START_SET,
                parse_attribute,
                "attribute",
            );
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
        let saved_doc_comments = context.tokens.take_doc_comment();
        let start_loc = context.tokens.start_loc();
        context.tokens.advance()?;
        let attributes_ = parse_comma_list(
            context,
            Tok::LBracket,
            Tok::RBracket,
            &ATTR_START_SET,
            parse_attribute,
            "attribute",
        );
        let end_loc = context.tokens.previous_end_loc();
        attributes_vec.push(spanned(
            context.tokens.file_hash(),
            start_loc,
            end_loc,
            attributes_,
        ));
        context.tokens.restore_doc_comment(saved_doc_comments);
    }
    Ok(attributes_vec)
}

//**************************************************************************************************
// Fields and Bindings
//**************************************************************************************************

// Parse an optional ellipis token. Consumes and returns the location of the token if present, and
// returns `Ok(None)` if not present.
//      Ellipsis = "..."?
fn parse_ellipsis_opt(context: &mut Context) -> Result<Option<Loc>, Box<Diagnostic>> {
    consume_optional_token_with_loc(context.tokens, Tok::PeriodPeriod)
}

// Parse an optional "mut" modifier token. Consumes and returns the location of the token if present
// and returns None otherwise.
//     MutOpt = "mut"?
fn parse_mut_opt(context: &mut Context) -> Result<Option<Loc>, Box<Diagnostic>> {
    // In migration mode, 'mut' is assumed to be an identifier that needsd escaping.
    if context.tokens.peek() == Tok::Mut {
        let start_loc = context.tokens.start_loc();
        context.tokens.advance()?;
        let end_loc = context.tokens.previous_end_loc();
        Ok(Some(make_loc(
            context.tokens.file_hash(),
            start_loc,
            end_loc,
        )))
    } else {
        Ok(None)
    }
}

// Parse a field name optionally followed by a colon and an expression argument:
//      ExpField = <Field> ( ":" <Exp> )?
fn parse_exp_field(context: &mut Context) -> Result<(Field, Exp), Box<Diagnostic>> {
    let f = parse_field(context)?;
    let arg = if match_token(context.tokens, Tok::Colon)? {
        parse_exp(context)?
    } else {
        sp(
            f.loc(),
            Exp_::Name(sp(f.loc(), NameAccessChain_::single(f.0))),
        )
    };
    Ok((f, arg))
}

// Parse a field name optionally followed by a colon and a binding:
//      BindField =
//          <Field> <":" <Bind>>?
//          | "mut" <Field>
//          | <Ellipsis>
//
// If the binding is not specified, the default is to use a variable
// with the same name as the field.
fn parse_bind_field(context: &mut Context) -> Result<Ellipsis<(Field, Bind)>, Box<Diagnostic>> {
    if let Some(loc) = parse_ellipsis_opt(context)? {
        return Ok(Ellipsis::Ellipsis(loc));
    }
    let mut_ = parse_mut_opt(context)?;
    let f = parse_field(context).or_else(|diag| match mut_ {
        Some(mut_loc)
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
        {
            report_name_migration(context, "mut", mut_loc);
            Ok(Field(sp(mut_.unwrap(), "mut".into())))
        }
        _ => Err(diag),
    })?;
    let arg = if mut_.is_some() {
        sp(f.loc(), Bind_::Var(mut_, Var(f.0)))
    } else if match_token(context.tokens, Tok::Colon)? {
        parse_bind(context)?
    } else {
        sp(f.loc(), Bind_::Var(None, Var(f.0)))
    };
    Ok(Ellipsis::Binder((f, arg)))
}

// Parse a binding:
//      Bind =
//          "mut"? <Var>
//          | <NameAccessChain> <OptionalTypeArgs> "{" Comma<BindField> "}"
//          | <NameAccessChain> <OptionalTypeArgs> "(" Comma<BindOrEllipsis> ")"
fn parse_bind(context: &mut Context) -> Result<Bind, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    if matches!(
        context.tokens.peek(),
        Tok::Identifier | Tok::RestrictedIdentifier | Tok::Mut
        // carve-out for migration with new keywords
        | Tok::Match | Tok::For | Tok::Enum | Tok::Type
    ) {
        let next_tok = context.tokens.lookahead()?;
        if !matches!(
            next_tok,
            Tok::LBrace | Tok::Less | Tok::ColonColon | Tok::LParen
        ) {
            let mut_ = parse_mut_opt(context)?;
            let v = parse_var(context).or_else(|diag| match mut_ {
                Some(mut_loc)
                    if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
                {
                    report_name_migration(context, "mut", mut_loc);
                    Ok(Var(sp(mut_.unwrap(), "mut".into())))
                }
                _ => Err(diag),
            })?;
            let v = Bind_::Var(mut_, v);
            let end_loc = context.tokens.previous_end_loc();
            return Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, v));
        }
    }
    // The item description specified here should include the special case above for
    // variable names, because if the current context cannot be parsed as a struct name
    // it is possible that the user intention was to use a variable name.
    let ty = parse_name_access_chain_with_tyarg_whitespace(context, /* macros */ false, || {
        "a variable or struct name"
    })?;
    let args = if context.tokens.peek() == Tok::LParen {
        let current_loc = current_token_loc(context.tokens);
        context.check_feature(
            context.current_package,
            FeatureGate::PositionalFields,
            current_loc,
        );
        let args = parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            &FIELD_BINDING_START_SET,
            parse_bind_or_ellipsis,
            "a field binding",
        );
        FieldBindings::Positional(args)
    } else {
        let args = parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            &FIELD_BINDING_START_SET,
            parse_bind_field,
            "a field binding",
        );
        FieldBindings::Named(args)
    };
    let end_loc = context.tokens.previous_end_loc();
    let unpack = Bind_::Unpack(Box::new(ty), args);
    Ok(spanned(
        context.tokens.file_hash(),
        start_loc,
        end_loc,
        unpack,
    ))
}

// Parse an inner field binding -- either a normal binding or an ellipsis:
//      EllipsisOrBind = <Bind> | <Ellipsis>
fn parse_bind_or_ellipsis(context: &mut Context) -> Result<Ellipsis<Bind>, Box<Diagnostic>> {
    if let Some(loc) = parse_ellipsis_opt(context)? {
        return Ok(Ellipsis::Ellipsis(loc));
    }
    let b = parse_bind(context)?;
    Ok(Ellipsis::Binder(b))
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
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION {
                &MIGRATION_PARAM_START_SET
            } else {
                &PARAM_START_SET
            },
            parse_bind,
            "a variable or structure binding",
        )
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, b))
}

// Parse a list of bindings for lambda.
//      LambdaBindList =
//          "|" Comma<BindList (":"  Type)?> "|"
fn parse_lambda_bind_list(context: &mut Context) -> Result<LambdaBindings, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let b = parse_comma_list(
        context,
        Tok::Pipe,
        Tok::Pipe,
        if context.env.edition(context.current_package) == Edition::E2024_MIGRATION {
            &MIGRATION_PARAM_START_SET
        } else {
            &PARAM_START_SET
        },
        |context| {
            let b = parse_bind_list(context)?;
            let ty_opt = if match_token(context.tokens, Tok::Colon)? {
                Some(parse_type(context)?)
            } else {
                None
            };
            Ok((b, ty_opt))
        },
        "a binding",
    );
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

// Checks if parsing of a sequence should continue after encountering an error.
fn should_continue_sequence_after_error(context: &mut Context, diag: Diagnostic) -> bool {
    context.add_diag(diag);
    // This is intended to handle a rather specific case when a valid sequence item is on the following line
    // from the parsing error. This is particularly useful for the IDE use case when a programmer starts
    // typing an incomplete (and unparsable) line right before the line containing a valid expression.
    // In this case, we would like to still report the error but try to avoid dropping the valid expression
    // itself, particularly as it might lead to unnecessary cascading errors to appear if this expression
    // is a variable declaration as in the example below where we want to avoid `_tmp1` being undefined
    // in the following lines.
    //
    // let v =
    // let _tmp1 = 42;
    // let _tmp2 = _tmp1 * param;
    // let _tmp3 = _tmp1 + param;

    if context.at_stop_set() {
        // don't continue if we are at the stop set
        return false;
    }
    let tok = context.tokens.peek();
    if context.tokens.last_token_preceded_by_eol()
        && (SEQ_ITEM_START_SET.contains(tok, context.tokens.content())
            //  ANY identfier can start a sequence item
            || tok == Tok::Identifier
            || tok == Tok::SyntaxIdentifier
            || tok == Tok::RestrictedIdentifier)
    {
        // if the last token was preceded by EOL, and it's in the start set for sequence items, continue
        // parsing the sequence
        return true;
    }
    false
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
        let tmp = parse_use_decl(
            DocComment::empty(),
            vec![],
            start_loc,
            Modifiers::empty(),
            context,
        )?;
        uses.push(tmp);
    }

    let mut seq: Vec<SequenceItem> = vec![];
    let mut last_semicolon_loc = None;
    let mut eopt = None;
    while context.tokens.peek() != Tok::RBrace {
        // this helps when a sequence contains a comma-separated list without the ending token (in
        // which case the parser would be likely fast-forwarded to EOF)
        context.stop_set.add(Tok::Semicolon);
        match parse_sequence_item(context) {
            Ok(item) => {
                context.stop_set.remove(Tok::Semicolon);
                if context.tokens.peek() == Tok::RBrace {
                    // If the sequence ends with an expression that is not
                    // followed by a semicolon, split out that expression
                    // from the rest of the SequenceItems.
                    if let SequenceItem_::Seq(e) = item.value {
                        eopt = Some(Spanned {
                            loc: item.loc,
                            value: e.value,
                        });
                    } else {
                        // we parsed a valid sequence - even though it should be followed by a
                        // semicolon, let's not drop it on the floor
                        seq.push(item);
                        context.add_diag(*unexpected_token_error(context.tokens, "';'"));
                    }
                    break;
                }
                seq.push(item);
                last_semicolon_loc = Some(current_token_loc(context.tokens));
                if let Err(diag) = consume_token(context.tokens, Tok::Semicolon) {
                    if should_continue_sequence_after_error(context, diag.as_ref().clone()) {
                        continue;
                    }
                    advance_separated_items_error(
                        context,
                        Tok::LBrace,
                        Tok::RBrace,
                        /* separator */ Tok::Semicolon,
                        /* for list */ true,
                        *diag,
                    );
                    if context.at_stop_set() {
                        break;
                    }
                }
            }
            Err(diag) => {
                context.stop_set.remove(Tok::Semicolon);
                if should_continue_sequence_after_error(context, diag.as_ref().clone()) {
                    continue;
                }
                let err_exp = sp(context.tokens.current_token_loc(), Exp_::UnresolvedError);
                let err_seq_item = SequenceItem_::Seq(Box::new(err_exp));
                seq.push(sp(context.tokens.current_token_loc(), err_seq_item));
                advance_separated_items_error(
                    context,
                    Tok::LBrace,
                    Tok::RBrace,
                    /* separator */ Tok::Semicolon,
                    /* for list */ true,
                    *diag,
                );
                if context.at_stop_set() {
                    break;
                }
            }
        }
    }
    // If we reached the stop set but did not find closing of the sequence (RBrace) and we need to
    // decide what to do. These are the two most likely possible scenarios:
    //
    // module 0x42::M {
    //   fun t() {
    //     let x = 0;
    //     use 0x1::M::foo;
    //     foo(x)
    //   }
    // }
    //
    // module 0x42::M {
    //   fun t() {
    //     let x = 0;
    //
    //   struct S {}
    // }
    //
    // In the first case we encounter stop set's `use` as incorrect inner definition, in the second
    // case, we encounter `struct` as a legit top-level definition after incomplete function
    // above. We cannot magically know which is which, though, at the point of reaching stop set,
    // but still need to make a decision on what to do, which at this point is to close the current
    // sequence and proceed with parsing top-level definition (assume the second scenario).
    if !context.at_stop_set() || context.tokens.at(Tok::RBrace) {
        context.advance(); // consume (the RBrace)
    }
    Ok((uses, seq, last_semicolon_loc, Box::new(eopt)))
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

// Parse an expression term:
//      Term =
//          "break" <BlockLabel>? <Exp>?
//          | "break" <BlockLabel>? "{" <Exp> "}"
//          | "continue" <BlockLabel>?
//          | "vector" ('<' Comma<Type> ">")? "[" Comma<Exp> "]"
//          | <Value>
//          | "(" Comma<Exp> ")"
//          | "(" <Exp> ":" <Type> ")"
//          | <BlockLabel> ":" <Exp>
//          | "{" <Sequence>
//          | "if" "(" <Exp> ")" <Exp> "else" (<BlockLabel> ":")? "{" <Exp> "}"
//          | "if" "(" <Exp> ")" (<BlockLabel> ":")? "{" <Exp> "}"
//          | "if" "(" <Exp> ")" <Exp> ("else" <Exp>)?
//          | "while" "(" <Exp> ")" (<BlockLabel> ":")? "{" <Exp> "}"
//          | "while" "(" <Exp> ")" <Exp> (SpecBlock)?
//          | "loop" <Exp>
//          | "loop" (<BlockLabel> ":")? "{" <Exp> "}"
//          | "return" <BlockLabel>? "{" <Exp> "}"
//          | "return" <BlockLabel>? <Exp>?
//          | "abort" "{" <Exp> "}"
//          | "abort" <Exp>
//          | "match" "(" <Exp> ")" "{" (<MatchArm> ",")+ "}"
#[growing_stack]
fn parse_term(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    const VECTOR_IDENT: &str = "vector";

    let start_loc = context.tokens.start_loc();
    let term = match context.tokens.peek() {
        tok if is_control_exp(context, tok) => {
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
            let tys_opt = parse_optional_type_args(context);
            let args_start_loc = context.tokens.start_loc();
            let args_ = parse_comma_list(
                context,
                Tok::LBracket,
                Tok::RBracket,
                &EXP_START_SET,
                parse_exp,
                "a vector argument expression",
            );
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
                .supports_feature(context.current_package, FeatureGate::Move2024Paths) =>
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
        Tok::Identifier | Tok::RestrictedIdentifier | Tok::SyntaxIdentifier => {
            parse_name_exp(context)?
        }
        // carve-out for migration with new keywords
        Tok::Mut | Tok::Match | Tok::For | Tok::Enum | Tok::Type
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
        {
            parse_name_exp(context)?
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
                } else {
                    if context.tokens.peek() != Tok::RParen {
                        consume_token(context.tokens, Tok::Comma)?;
                    }
                    let mut es = parse_comma_list_after_start(
                        context,
                        list_loc,
                        Tok::LParen,
                        Tok::RParen,
                        &EXP_START_SET,
                        parse_exp,
                        "an expression",
                    );
                    if es.is_empty() {
                        Exp_::Parens(Box::new(e))
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
            let spec_string = consume_spec_string(context)?;
            Exp_::Spec(spec_string)
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

fn is_control_exp(context: &mut Context, tok: Tok) -> bool {
    matches!(
        tok,
        Tok::Break
            | Tok::Continue
            | Tok::If
            | Tok::While
            | Tok::Loop
            | Tok::Return
            | Tok::Abort
            | Tok::BlockLabel
    ) || (matches!(tok, Tok::Match)
        && context
            .env
            .supports_feature(context.current_package, FeatureGate::Move2024Keywords)
        && context.env.edition(context.current_package) != Edition::E2024_MIGRATION)
}

// An identifier with a leading ', used to label blocks and control flow
//      BlockLabel = <BlockIdentifierValue>
// roughly "'"<Identifier> but whitespace sensitive
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
// This also aplies to any named block
// e.g.           if (cond) e1 else 'a: { e2 } + 1
// should be,    (if (cond) e1 else 'a: { e2 }) + 1
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
            Tok::BlockLabel => {
                let start_loc = context.tokens.start_loc();
                let label = parse_block_label(context)?;
                consume_token(context.tokens, Tok::Colon)?;
                let (e, ends_in_block) = parse_exp_or_sequence(context)?;
                let end_loc = context.tokens.previous_end_loc();
                let labeled_ = Exp_::Labeled(label, Box::new(e));
                let labeled = spanned(context.tokens.file_hash(), start_loc, end_loc, labeled_);
                Ok((labeled, ends_in_block))
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
                let start_loc = context.tokens.start_loc();
                let spec = consume_spec_string(context)?;
                let loc = make_loc(
                    context.tokens.file_hash(),
                    start_loc,
                    context.tokens.previous_end_loc(),
                );

                let spec_seq = sp(loc, SequenceItem_::Seq(Box::new(sp(loc, Exp_::Spec(spec)))));
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
            let (e, ends_in_block) = if !at_start_of_exp(context) {
                (None, false)
            } else {
                let (e, ends_in_block) = parse_exp_or_sequence(context)?;
                (Some(Box::new(e)), ends_in_block)
            };
            (Exp_::Abort(e), ends_in_block)
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
        Tok::BlockLabel => {
            let name = parse_block_label(context)?;
            consume_token(context.tokens, Tok::Colon)?;
            let (e, ends_in_block) = if is_control_exp(context, context.tokens.peek()) {
                parse_control_exp(context)?
            } else {
                parse_exp_or_sequence(context)?
            };
            (Exp_::Labeled(name, Box::new(e)), ends_in_block)
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
//          | <NameAccessChain> "!" <OptionalTypeArgs> "(" Comma<Exp> ")"
//          | <NameAccessChain> <OptionalTypeArgs>
fn parse_name_exp(context: &mut Context) -> Result<Exp_, Box<Diagnostic>> {
    let name = parse_name_access_chain(
        context,
        /* macros */ true,
        /* tyargs */ true,
        || panic!("parse_name_exp with something other than a ModuleAccess"),
    )?;

    match context.tokens.peek() {
        _ if name.value.is_macro().is_some() => {
            // if in a macro, we must have a call
            let rhs = parse_call_args(context);
            Ok(Exp_::Call(name, rhs))
        }

        // Pack: "{" Comma<ExpField> "}"
        Tok::LBrace => {
            let fs = parse_comma_list(
                context,
                Tok::LBrace,
                Tok::RBrace,
                &TokenSet::from([Tok::Identifier, Tok::RestrictedIdentifier]),
                parse_exp_field,
                "a field expression",
            );
            Ok(Exp_::Pack(name, fs))
        }

        // Call: "(" Comma<Exp> ")"
        Tok::LParen => {
            debug_assert!(name.value.is_macro().is_none());
            let rhs = parse_call_args(context);
            Ok(Exp_::Call(name, rhs))
        }

        // Other name reference...
        _ => Ok(Exp_::Name(name)),
    }
}

// Parse the arguments to a call: "(" Comma<Exp> ")"
fn parse_call_args(context: &mut Context) -> Spanned<Vec<Exp>> {
    let (loc, args) = with_loc!(
        context,
        parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            &EXP_START_SET,
            parse_exp,
            "a call argument expression",
        )
    );
    sp(loc, args)
}

// Parses a series of match arms, such as for a match block body "{" (<MatchArm>,)+ "}"
fn parse_match_arms(context: &mut Context) -> Result<Spanned<Vec<MatchArm>>, Box<Diagnostic>> {
    let mut match_arms_start_set = VALUE_START_SET.clone();
    match_arms_start_set.add_all(&[Tok::LParen, Tok::Mut, Tok::Identifier]);
    ok_with_loc!(
        context,
        parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            &match_arms_start_set,
            parse_match_arm,
            "a call argument expression",
        )
    )
}

// Parses a match arm:
//   <MatchArm> = <MatchPat> ( "if" "(" <Exp>")" )? "=>" ("{" <Exp> "}" | <Exp>)
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
        if let Err(diag) = consume_token(context.tokens, Tok::EqualGreater) {
            // report incomplete pattern so that auto-completion can work
            context.add_diag(*diag);
            MatchArm_ {
                pattern,
                guard,
                rhs: Box::new(sp(Loc::invalid(), Exp_::UnresolvedError)),
            }
        } else {
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
        }
    })
}

// Parses a match pattern:
//   <MatchPat> = <OptAtPat> ("|" <MatchPat>)?
//   <OptAtPat> = (<Identifier> "@")? <CtorPat>
//   <CtorPat>  = "(" <MatchPat> ")"
//              | <NameAccessChain> <OptionalTypeArgs> "{" Comma<PatField> "}"
//              | <NameAccessChain> <OptionalTypeArgs> "(" Comma<MatchPat> ")"
//              | <NameAccessChain> <OptionalTypeArgs>
//   <PatField> = <Field> ( ":" <MatchPat> )?

fn parse_match_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
    const INVALID_PAT_ERROR_MSG: &str = "Invalid pattern";
    const WILDCARD_AT_ERROR_MSG: &str = "Cannot use '_' as a binder in an '@' pattern";

    use MatchPattern_ as MP;

    fn parse_ctor_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
        match context.tokens.peek() {
            Tok::LParen => {
                context.tokens.advance()?;
                let pat = parse_match_pattern(context);
                consume_token(context.tokens, Tok::RParen)?;
                pat
            }
            t @ (Tok::Mut | Tok::Identifier | Tok::NumValue)
                if !matches!(t, Tok::NumValue)
                    || matches!(context.tokens.lookahead(), Ok(Tok::ColonColon)) =>
            {
                ok_with_loc!(context, {
                    let mut_ = parse_mut_opt(context)?;
                    let name_access_chain = parse_name_access_chain(
                        context,
                        /* macros */ false,
                        /* tyargs */ true,
                        || "a pattern entry",
                    )?;

                    fn report_invalid_mut(context: &mut Context, mut_: Option<Loc>) {
                        if let Some(loc) = mut_ {
                            let diag = diag!(
                                Syntax::UnexpectedToken,
                                (loc, "Invalid 'mut' keyword on non-variable pattern")
                            );
                            context.add_diag(diag);
                        }
                    }

                    match context.tokens.peek() {
                        Tok::LParen => {
                            let mut pattern_start_set = VALUE_START_SET.clone();
                            pattern_start_set.add_all(&[
                                Tok::PeriodPeriod,
                                Tok::LParen,
                                Tok::Mut,
                                Tok::Identifier,
                            ]);
                            let (loc, patterns) = with_loc!(
                                context,
                                parse_comma_list(
                                    context,
                                    Tok::LParen,
                                    Tok::RParen,
                                    &pattern_start_set,
                                    parse_positional_field_pattern,
                                    "a pattern",
                                )
                            );
                            report_invalid_mut(context, mut_);
                            MP::PositionalConstructor(name_access_chain, sp(loc, patterns))
                        }
                        Tok::LBrace => {
                            let (loc, patterns) = with_loc!(
                                context,
                                parse_comma_list(
                                    context,
                                    Tok::LBrace,
                                    Tok::RBrace,
                                    &TokenSet::from([Tok::PeriodPeriod, Tok::Mut, Tok::Identifier]),
                                    parse_field_pattern,
                                    "a field pattern",
                                )
                            );
                            report_invalid_mut(context, mut_);
                            MP::FieldConstructor(name_access_chain, sp(loc, patterns))
                        }
                        _ => MP::Name(mut_, name_access_chain),
                    }
                })
            }
            _ => {
                if let Some(value) = maybe_parse_value(context)? {
                    Ok(sp(value.loc, MP::Literal(value)))
                } else {
                    Err(Box::new(diag!(
                        Syntax::UnexpectedToken,
                        (context.tokens.current_token_loc(), INVALID_PAT_ERROR_MSG)
                    )))
                }
            }
        }
    }

    fn parse_positional_field_pattern(
        context: &mut Context,
    ) -> Result<Ellipsis<MatchPattern>, Box<Diagnostic>> {
        if let Some(loc) = parse_ellipsis_opt(context)? {
            return Ok(Ellipsis::Ellipsis(loc));
        }

        parse_match_pattern(context).map(Ellipsis::Binder)
    }

    fn parse_field_pattern(
        context: &mut Context,
    ) -> Result<Ellipsis<(Field, MatchPattern)>, Box<Diagnostic>> {
        const INVALID_MUT_ERROR_MSG: &str = "'mut' modifier can only be used on variable bindings";

        if let Some(loc) = parse_ellipsis_opt(context)? {
            return Ok(Ellipsis::Ellipsis(loc));
        }

        let mut_ = parse_mut_opt(context)?;
        let field = parse_field(context)?;
        let pattern = if match_token(context.tokens, Tok::Colon)? {
            if let Some(loc) = mut_ {
                return Err(Box::new(diag!(
                    Syntax::UnexpectedToken,
                    (loc, INVALID_MUT_ERROR_MSG)
                )));
            }
            parse_match_pattern(context)?
        } else {
            sp(
                field.loc(),
                MP::Name(mut_, sp(field.loc(), NameAccessChain_::single(field.0))),
            )
        };
        Ok(Ellipsis::Binder((field, pattern)))
    }

    fn parse_optional_at_pattern(context: &mut Context) -> Result<MatchPattern, Box<Diagnostic>> {
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
                        MP::At(binder, Box::new(rhs))
                    })
                }
            }
            _ => parse_ctor_pattern(context),
        }
    }

    ok_with_loc!(context, {
        let lhs = parse_optional_at_pattern(context)?;
        if matches!(context.tokens.peek(), Tok::Pipe) {
            context.tokens.advance()?;
            let rhs = parse_match_pattern(context)?;
            MP::Or(Box::new(lhs), Box::new(rhs))
        } else {
            lhs.value
        }
    })
}

// Parse the arguments to an index: "[" Comma<Exp> "]"
fn parse_index_args(context: &mut Context) -> Spanned<Vec<Exp>> {
    let start_loc = context.tokens.start_loc();
    let args = parse_comma_list(
        context,
        Tok::LBracket,
        Tok::RBracket,
        &EXP_START_SET,
        parse_exp,
        "an index access expression",
    );
    let end_loc = context.tokens.previous_end_loc();
    spanned(context.tokens.file_hash(), start_loc, end_loc, args)
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
            | Tok::Match
            | Tok::Loop
            | Tok::Return
            | Tok::While
            | Tok::BlockLabel
    )
}

// Parse an expression:
//      Exp =
//            <LambdaBindList> <Exp>
//          | <LambdaBindList> "->" <Type> "{" <Sequence>
//          | <Quantifier>                  spec only
//          | <BinOpExp>
//          | <UnaryExp> "=" <Exp>
fn parse_exp(context: &mut Context) -> Result<Exp, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let exp = match context.tokens.peek() {
        tok @ Tok::PipePipe | tok @ Tok::Pipe => {
            let bindings = if tok == Tok::PipePipe {
                let loc = current_token_loc(context.tokens);
                consume_token(context.tokens, Tok::PipePipe)?;
                sp(loc, vec![])
            } else {
                parse_lambda_bind_list(context)?
            };
            let (ret_ty_opt, body) = if context.tokens.peek() == Tok::MinusGreater {
                context.tokens.advance()?;
                let ret_ty = parse_type(context)?;
                let label_opt = if matches!(context.tokens.peek(), Tok::BlockLabel) {
                    let start_loc = context.tokens.start_loc();
                    let label = parse_block_label(context)?;
                    consume_token(context.tokens, Tok::Colon)?;
                    Some((start_loc, label))
                } else {
                    None
                };
                let start_loc = context.tokens.start_loc();
                consume_token(context.tokens, Tok::LBrace)?;
                let block_ = Exp_::Block(parse_sequence(context)?);
                let end_loc = context.tokens.previous_end_loc();
                let block = spanned(context.tokens.file_hash(), start_loc, end_loc, block_);
                let body = if let Some((lbl_start_loc, label)) = label_opt {
                    let labeled_ = Exp_::Labeled(label, Box::new(block));
                    spanned(context.tokens.file_hash(), lbl_start_loc, end_loc, labeled_)
                } else {
                    block
                };
                (Some(ret_ty), body)
            } else {
                (None, parse_exp(context)?)
            };
            Exp_::Lambda(bindings, ret_ty_opt, Box::new(body))
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
        Tok::As => 6,
        Tok::PeriodPeriod => 7,
        Tok::Pipe => 8,
        Tok::Caret => 9,
        Tok::Amp => 10,
        Tok::LessLess => 11,
        Tok::GreaterGreater => 11,
        Tok::Plus => 12,
        Tok::Minus => 12,
        Tok::Star => 13,
        Tok::Slash => 13,
        Tok::Percent => 13,
        _ => 0, // anything else is not a binary operator
    }
}

// Parse a binary operator expression:
//      BinOpExp =
//          <BinOpExp> <BinOp> <BinOpExp>
//          | <BinOpExp> "as" <Type> // in some sense, the lowest precedence binop
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
#[growing_stack]
fn parse_binop_exp(context: &mut Context, lhs: Exp, min_prec: u32) -> Result<Exp, Box<Diagnostic>> {
    let mut result = lhs;
    let mut next_tok_prec = get_precedence(context.tokens.peek());

    while next_tok_prec >= min_prec {
        // Parse the operator.
        let op_start_loc = context.tokens.start_loc();
        let op_token = context.tokens.peek();
        context.tokens.advance()?;
        let op_end_loc = context.tokens.previous_end_loc();

        if op_token == Tok::As {
            let ty = parse_type_(context, /* whitespace_sensitive_ty_args */ true)?;
            let start_loc = result.loc.start() as usize;
            let end_loc = context.tokens.previous_end_loc();
            let e_ = Exp_::Cast(Box::new(result), ty);
            result = spanned(context.tokens.file_hash(), start_loc, end_loc, e_);
            next_tok_prec = get_precedence(context.tokens.peek());
            continue;
        }

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
        let first_token_loc = current_token_loc(context.tokens);
        let exp = match context.tokens.peek() {
            Tok::Period => {
                context.advance();
                let loc = current_token_loc(context.tokens);
                match context.tokens.peek() {
                    Tok::NumValue | Tok::NumTypedValue
                        if context.check_feature(
                            context.current_package,
                            FeatureGate::PositionalFields,
                            loc,
                        ) =>
                    {
                        let contents = context.tokens.content();
                        context.advance();
                        match parse_u8(contents) {
                            Ok((parsed, NumberFormat::Decimal)) => {
                                let field_access = Name::new(loc, format!("{parsed}").into());
                                Exp_::Dot(Box::new(lhs), first_token_loc, field_access)
                            }
                            Ok((_, NumberFormat::Hex)) => {
                                let msg = "Invalid field access. Expected a decimal number but was given a hexadecimal";
                                let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                                diag.add_note("Positional fields must be a decimal number in the range [0 .. 255] and not be typed, e.g. `0`");
                                context.add_diag(diag);
                                // Continue on with the parsing
                                let field_access = Name::new(loc, contents.into());
                                Exp_::Dot(Box::new(lhs), first_token_loc, field_access)
                            }
                            Err(_) => {
                                let msg = format!(
                                    "Invalid field access. Expected a number less than or equal to {}",
                                    u8::MAX
                                );
                                let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                                diag.add_note("Positional fields must be a decimal number in the range [0 .. 255] and not be typed, e.g. `0`");
                                context.add_diag(diag);
                                // Continue on with the parsing
                                let field_access = Name::new(loc, contents.into());
                                Exp_::Dot(Box::new(lhs), first_token_loc, field_access)
                            }
                        }
                    }
                    _ => match parse_identifier(context) {
                        Err(_) => {
                            // if it's neither a number (checked above) nor identifier, it conveys
                            // more information to the developer to signal that both are a
                            // possibility here (rather than just identifier which would be signaled
                            // if we kept the returned diagnostic)
                            context.add_diag(*unexpected_token_error(
                                context.tokens,
                                "an identifier or a decimal number",
                            ));
                            if context.env.ide_mode() {
                                Exp_::DotUnresolved(first_token_loc, Box::new(lhs))
                            } else {
                                Exp_::UnresolvedError
                            }
                        }
                        Ok(n) => {
                            if is_start_of_call_after_function_name(context, &n) {
                                let (is_macro, tys) =
                                    parse_macro_opt_and_tyargs_opt(context, false, n.loc);
                                let tys = tys.map(|t| t.value);
                                let args = parse_call_args(context);
                                Exp_::DotCall(
                                    Box::new(lhs),
                                    first_token_loc,
                                    n,
                                    is_macro,
                                    tys,
                                    args,
                                )
                            } else {
                                Exp_::Dot(Box::new(lhs), first_token_loc, n)
                            }
                        }
                    },
                }
            }
            Tok::LBracket => {
                let index_args = parse_index_args(context);

                Exp_::Index(Box::new(lhs), index_args)
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
    let peeked = context.tokens.peek();
    (peeked == Tok::Less && context.at_end(n.loc))
        || peeked == Tok::LParen
        || peeked == Tok::Exclaim
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
                Ok(parse_comma_list(
                    context,
                    Tok::LBrace,
                    Tok::RBrace,
                    &EXP_START_SET,
                    parse_exp,
                    "a trigger expresssion",
                ))
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
        make_builtin_call(ty.loc, symbol!("$spec_domain"), vec![])
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

fn make_builtin_call(loc: Loc, name: Symbol, args: Vec<Exp>) -> Exp {
    let maccess = sp(loc, NameAccessChain_::single(sp(loc, name)));
    sp(loc, Exp_::Call(maccess, sp(loc, args)))
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
    parse_type_(context, /* whitespace_sensitive_ty_args */ false)
}

fn parse_type_(
    context: &mut Context,
    whitespace_sensitive_ty_args: bool,
) -> Result<Type, Box<Diagnostic>> {
    let start_loc = context.tokens.start_loc();
    let t = match context.tokens.peek() {
        Tok::LParen => {
            context.stop_set.union(&TYPE_STOP_SET);
            let mut ts = parse_comma_list(
                context,
                Tok::LParen,
                Tok::RParen,
                &TYPE_START_SET,
                parse_type,
                "a type",
            );
            context.stop_set.difference(&TYPE_STOP_SET);
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
        tok @ Tok::PipePipe | tok @ Tok::Pipe => {
            let args = if tok == Tok::PipePipe {
                context.tokens.advance()?;
                vec![]
            } else {
                context.stop_set.union(&TYPE_STOP_SET);
                let list = parse_comma_list(
                    context,
                    Tok::Pipe,
                    Tok::Pipe,
                    &TYPE_START_SET,
                    parse_type,
                    "a type",
                );
                context.stop_set.difference(&TYPE_STOP_SET);
                list
            };
            let result = if context
                .tokens
                .edition()
                .supports(FeatureGate::Move2024Keywords)
            {
                // 2024 syntax
                if context.tokens.peek() == Tok::MinusGreater {
                    context.tokens.advance()?;
                    parse_type(context)?
                } else {
                    spanned(
                        context.tokens.file_hash(),
                        start_loc,
                        context.tokens.start_loc(),
                        Type_::Unit,
                    )
                }
            } else {
                // legacy spec syntax
                parse_type(context)?
            };
            return Ok(spanned(
                context.tokens.file_hash(),
                start_loc,
                context.tokens.previous_end_loc(),
                Type_::Fun(args, Box::new(result)),
            ));
        }
        _ => {
            if context.at_stop_set() {
                context.add_diag(*unexpected_token_error(context.tokens, "a type name"));
                Type_::UnresolvedError
            } else {
                let tn = if whitespace_sensitive_ty_args {
                    parse_name_access_chain(
                        context,
                        /* macros */ false,
                        /* tyargs */ true,
                        || "a type name",
                    )?
                } else {
                    parse_name_access_chain_with_tyarg_whitespace(
                        context,
                        /* macros */ false,
                        || "a type name",
                    )?
                };
                Type_::Apply(Box::new(tn))
            }
        }
    };
    let end_loc = context.tokens.previous_end_loc();
    Ok(spanned(context.tokens.file_hash(), start_loc, end_loc, t))
}

// Parse an optional list of type arguments.
//    OptionalTypeArgs = '<' Comma<Type> ">" | <empty>
fn parse_optional_type_args(context: &mut Context) -> Option<Vec<Type>> {
    if context.tokens.peek() == Tok::Less {
        context.stop_set.union(&TYPE_STOP_SET);
        let list = Some(parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            &TYPE_START_SET,
            parse_type,
            "a type",
        ));
        context.stop_set.difference(&TYPE_STOP_SET);
        list
    } else {
        None
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
//          <SyntaxIdentifier> <Constraint>?
//        | <Identifier> <Constraint>?
//      Constraint =
//          ":" <Ability> (+ <Ability>)*
fn parse_type_parameter(context: &mut Context) -> Result<(Name, Vec<Ability>), Box<Diagnostic>> {
    let n = if context.tokens.peek() == Tok::SyntaxIdentifier {
        parse_syntax_identifier(context)?
    } else {
        parse_identifier(context)?
    };

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
fn parse_optional_type_parameters(context: &mut Context) -> Vec<(Name, Vec<Ability>)> {
    if context.tokens.peek() == Tok::Less {
        context.stop_set.union(&TYPE_STOP_SET);
        let list = parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            &TokenSet::from([
                Tok::Identifier,
                Tok::SyntaxIdentifier,
                Tok::RestrictedIdentifier,
            ]),
            parse_type_parameter,
            "a type parameter",
        );
        context.stop_set.difference(&TYPE_STOP_SET);
        list
    } else {
        vec![]
    }
}

// Parse optional datatype type parameters:
//    DatatypeTypeParameter = '<' Comma<TypeParameterWithPhantomDecl> ">" | <empty>
fn parse_datatype_type_parameters(context: &mut Context) -> Vec<DatatypeTypeParameter> {
    if context.tokens.peek() == Tok::Less {
        context.stop_set.union(&TYPE_STOP_SET);
        let list = parse_comma_list(
            context,
            Tok::Less,
            Tok::Greater,
            &TokenSet::from([Tok::Identifier, Tok::RestrictedIdentifier]),
            parse_datatype_type_parameter,
            "a type parameter",
        );
        context.stop_set.difference(&TYPE_STOP_SET);
        list
    } else {
        vec![]
    }
}

// Parse type parameter with optional phantom declaration:
//   TypeParameterWithPhantomDecl = "phantom"? <TypeParameter>
fn parse_datatype_type_parameter(
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
//          ("{" <Sequence> "}" | ";")
//
fn parse_function_decl(
    doc: DocComment,
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<Function, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
        macro_,
    } = modifiers;

    // "fun" <FunctionDefName>
    consume_token(context.tokens, Tok::Fun)?;
    let name = FunctionName(parse_identifier(context)?);

    context.stop_set.add(Tok::LParen);
    context.stop_set.add(Tok::LBrace);

    let type_parameters = parse_optional_type_parameters(context);
    context.stop_set.remove(Tok::LParen);

    // "(" Comma<Parameter> ")"
    let parameters = parse_comma_list(
        context,
        Tok::LParen,
        Tok::RParen,
        if context.env.edition(context.current_package) == Edition::E2024_MIGRATION {
            &MIGRATION_PARAM_START_SET
        } else {
            &PARAM_START_SET
        },
        parse_parameter,
        "a function parameter",
    );

    let return_type = parse_ret_type(context, name)
        .inspect_err(|diag| {
            context.advance_until_stop_set(Some(*diag.clone()));
        })
        .ok();

    context.stop_set.remove(Tok::LBrace);

    let body = parse_body(context, native)
        .inspect_err(|diag| {
            context.advance_until_stop_set(Some(*diag.clone()));
        })
        .ok();

    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );

    Ok(Function {
        doc,
        attributes,
        loc,
        visibility: visibility.unwrap_or(Visibility::Internal),
        entry,
        macro_,
        signature: FunctionSignature {
            type_parameters,
            parameters,
            return_type: return_type.unwrap_or_else(|| sp(name.loc(), Type_::UnresolvedError)),
        },
        name,
        body: body.unwrap_or_else(|| {
            let loc = context.tokens.current_token_loc();
            let seq_exp = Box::new(Some(sp(loc, Exp_::UnresolvedError)));
            sp(loc, FunctionBody_::Defined((vec![], vec![], None, seq_exp)))
        }),
    })
}

// Parse a function parameter:
//      Parameter = "mut"? <Var> ":" <Type>
fn parse_parameter(context: &mut Context) -> Result<(Mutability, Var, Type), Box<Diagnostic>> {
    let mut_ = parse_mut_opt(context)?;
    let v = parse_var(context).or_else(|diag| match mut_ {
        Some(mut_loc)
            if context.env.edition(context.current_package) == Edition::E2024_MIGRATION =>
        {
            report_name_migration(context, "mut", mut_loc);
            Ok(Var(sp(mut_.unwrap(), "mut".into())))
        }
        _ => Err(diag),
    })?;
    consume_token(context.tokens, Tok::Colon)?;
    let t = parse_type(context)?;
    Ok((mut_, v, t))
}

// (":" <Type>)?
fn parse_ret_type(context: &mut Context, name: FunctionName) -> Result<Type, Box<Diagnostic>> {
    if match_token(context.tokens, Tok::Colon)? {
        parse_type(context)
    } else {
        Ok(sp(name.loc(), Type_::Unit))
    }
}

fn parse_body(context: &mut Context, native: Option<Loc>) -> Result<FunctionBody, Box<Diagnostic>> {
    match native {
        Some(loc) => {
            if let Err(diag) = consume_token(context.tokens, Tok::Semicolon) {
                context.advance_until_stop_set(Some(*diag));
            }
            Ok(sp(loc, FunctionBody_::Native))
        }
        _ => {
            let start_loc = context.tokens.start_loc();
            let seq = if context.tokens.peek() == Tok::LBrace {
                match consume_token(context.tokens, Tok::LBrace) {
                    Ok(_) => parse_sequence(context)?,
                    Err(diag) => {
                        // error advancing past opening brace - assume sequence (likely first)
                        // parsing problem and try skipping it
                        advance_separated_items_error(
                            context,
                            Tok::LBrace,
                            Tok::RBrace,
                            /* separator */ Tok::Semicolon,
                            /* for list */ true,
                            *diag,
                        );
                        let _ = match_token(context.tokens, Tok::RBrace);
                        (
                            vec![],
                            vec![],
                            None,
                            Box::new(Some(sp(
                                context.tokens.current_token_loc(),
                                Exp_::UnresolvedError,
                            ))),
                        )
                    }
                }
            } else {
                // not even opening brace - not much of a body to parse
                let diag = unexpected_token_error(context.tokens, "'{'");
                context.advance_until_stop_set(Some(*diag));
                (
                    vec![],
                    vec![],
                    None,
                    Box::new(Some(sp(
                        context.tokens.current_token_loc(),
                        Exp_::UnresolvedError,
                    ))),
                )
            };
            let end_loc = context.tokens.previous_end_loc();
            Ok(sp(
                make_loc(context.tokens.file_hash(), start_loc, end_loc),
                FunctionBody_::Defined(seq),
            ))
        }
    }
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
// Where the two "has" statements are mutually exclusive -- an enum cannot be declared with
// both infix and postfix ability declarations.
fn parse_enum_decl(
    doc: DocComment,
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<EnumDefinition, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
        macro_,
    } = modifiers;

    check_no_modifier(context, ENTRY_MODIFIER, entry, "enum");
    check_no_modifier(context, MACRO_MODIFIER, macro_, "enum");
    check_no_modifier(context, NATIVE_MODIFIER, native, "enum");
    check_enum_visibility(visibility, context);

    consume_token(context.tokens, Tok::Enum)?;

    // <EnumDefName>
    let name = DatatypeName(parse_identifier(context)?);
    let type_parameters = parse_datatype_type_parameters(context);

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
        doc,
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
        &TokenSet::from([Tok::Identifier, Tok::RestrictedIdentifier]),
        parse_enum_variant_decl,
        "a variant",
    );
    Ok(variants)
}

// Parse an enum variant definition:
//      VariantDecl = <Identifier> ("{" Comma<FieldAnnot> "}" | "(" Comma<PosField> ")")
fn parse_enum_variant_decl(context: &mut Context) -> Result<VariantDefinition, Box<Diagnostic>> {
    let doc = match_doc_comments(context);
    let start_loc = context.tokens.start_loc();
    let name = parse_identifier(context)?;
    let fields = parse_enum_variant_fields(context)?;
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(VariantDefinition {
        doc,
        loc,
        name: VariantName(name),
        fields,
    })
}

fn parse_enum_variant_fields(context: &mut Context) -> Result<VariantFields, Box<Diagnostic>> {
    match context.tokens.peek() {
        Tok::LParen => {
            let current_package = context.current_package;
            let loc = current_token_loc(context.tokens);
            context.check_feature(current_package, FeatureGate::PositionalFields, loc);

            let list = parse_comma_list(
                context,
                Tok::LParen,
                Tok::RParen,
                &TYPE_START_SET,
                parse_positional_field,
                "a type",
            );
            Ok(VariantFields::Positional(list))
        }
        Tok::LBrace => {
            let fields = parse_comma_list(
                context,
                Tok::LBrace,
                Tok::RBrace,
                &TokenSet::from([Tok::Identifier, Tok::RestrictedIdentifier]),
                parse_field_annot,
                "a field",
            );
            Ok(VariantFields::Named(fields))
        }
        _ => Ok(VariantFields::Empty),
    }
}

fn check_enum_visibility(visibility: Option<Visibility>, context: &mut Context) {
    let current_package = context.current_package;
    // NB this could be an if-let but we will eventually want the match for other vis. support.
    match &visibility {
        Some(Visibility::Public(loc)) => {
            context.check_feature(current_package, FeatureGate::Enums, *loc);
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
            context.add_diag(err);
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
// Where the two "has" statements are mutually exclusive -- a struct cannot be declared with
// both infix and postfix ability declarations.
fn parse_struct_decl(
    doc: DocComment,
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<StructDefinition, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
        macro_,
    } = modifiers;

    check_struct_visibility(visibility, context);

    check_no_modifier(context, ENTRY_MODIFIER, entry, "struct");
    check_no_modifier(context, MACRO_MODIFIER, macro_, "struct");

    consume_token(context.tokens, Tok::Struct)?;

    // <StructDefName>
    let name = DatatypeName(parse_identifier(context)?);

    context
        .stop_set
        .add_all(&[Tok::LBrace, Tok::LParen, Tok::Semicolon]);
    let type_parameters = parse_datatype_type_parameters(context);

    let mut infix_ability_declaration_loc =
        if context.tokens.peek() == Tok::Identifier && context.tokens.content() == "has" {
            Some(current_token_loc(context.tokens))
        } else {
            None
        };
    // is the `has` keyword for infix abilities present
    let infix_ability_has_keyword = infix_ability_declaration_loc.is_some();

    let mut abilities = if infix_ability_declaration_loc.is_some() {
        parse_infix_ability_declarations(context)
            .inspect_err(|diag| {
                // if parsing failed, assume no abilities present even if `has` keyword was present
                infix_ability_declaration_loc = None;
                context.advance_until_stop_set(Some(*diag.clone()));
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    // we are supposed to start parsing struct fields here
    if !context
        .tokens
        .at_set(&TokenSet::from(&[Tok::LBrace, Tok::LParen, Tok::Semicolon]))
    {
        let unexpected_loc = current_token_loc(context.tokens);
        let msg = if infix_ability_has_keyword {
            format!(
                "Unexpected '{}'. Expected struct fields or ';' for a native struct",
                context.tokens.peek()
            )
        } else {
            format!(
                "Unexpected '{}'. Expected struct fields, 'has' to start abilities declaration, \
                 or ';' for a native struct",
                context.tokens.peek()
            )
        };
        let diag = diag!(Syntax::UnexpectedToken, (unexpected_loc, msg));
        context.add_diag(diag);
    }

    if !context.at_stop_set() {
        // try advancing until we reach fields defnition or the "outer" stop set
        context.advance_until_stop_set(None);
    }

    context
        .stop_set
        .remove_all(&[Tok::LBrace, Tok::LParen, Tok::Semicolon]);

    let mut fields = None;
    if !context.at_stop_set() {
        fields = parse_struct_body(
            context,
            native,
            infix_ability_declaration_loc,
            &mut abilities,
        )
        .inspect_err(|diag| {
            context.advance_until_stop_set(Some(*diag.clone()));
        })
        .ok();
    }

    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    Ok(StructDefinition {
        doc,
        attributes,
        loc,
        abilities,
        name,
        type_parameters,
        fields: fields.unwrap_or(StructFields::Native(loc)),
    })
}

// Parse either just semicolon (for native structs) or fields and (optional) postfix abilities
fn parse_struct_body(
    context: &mut Context,
    native: Option<Loc>,
    infix_ability_declaration_loc: Option<Loc>,
    abilities: &mut Vec<Ability>,
) -> Result<StructFields, Box<Diagnostic>> {
    Ok(match native {
        Some(loc) => {
            consume_token(context.tokens, Tok::Semicolon)?;
            StructFields::Native(loc)
        }
        _ => {
            let fields = parse_struct_fields(context)?;
            parse_postfix_ability_declarations(infix_ability_declaration_loc, abilities, context)?;
            fields
        }
    })
}

// Parse a field annotated with a type:
//      FieldAnnot = <DocComments> <Field> ":" <Type>
fn parse_field_annot(context: &mut Context) -> Result<(DocComment, Field, Type), Box<Diagnostic>> {
    let doc = match_doc_comments(context);
    let f = parse_field(context)?;
    consume_token(context.tokens, Tok::Colon)?;
    let st = parse_type(context)?;
    Ok((doc, f, st))
}

// Parse a positional struct field:
//      PosField = <DocComments> <Type>
fn parse_positional_field(context: &mut Context) -> Result<(DocComment, Type), Box<Diagnostic>> {
    let doc = match_doc_comments(context);
    if matches!(
        (context.tokens.peek(), context.tokens.lookahead()),
        (Tok::Identifier, Ok(Tok::Colon))
    ) {
        let diag = diag!(
            Syntax::UnexpectedToken,
            (
                context.tokens.current_token_loc(),
                "Cannot use named fields in a positional definition"
            )
        );
        context.add_diag(diag);
        // advance to (presumably) the actual type
        context.tokens.advance()?;
        context.tokens.advance()?;
    }
    Ok((doc, parse_type(context)?))
}

// Parse a infix ability declaration:
//     "has" <Ability> (, <Ability>)+
fn parse_infix_ability_declarations(
    context: &mut Context,
) -> Result<Vec<Ability>, Box<Diagnostic>> {
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
    )
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
        context.check_feature(
            context.current_package,
            FeatureGate::PostFixAbilities,
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
            context.add_diag(diag!(
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
        let current_package = context.current_package;
        let loc = current_token_loc(context.tokens);
        context.check_feature(current_package, FeatureGate::PositionalFields, loc);

        context.stop_set.union(&TYPE_STOP_SET);
        let list = parse_comma_list(
            context,
            Tok::LParen,
            Tok::RParen,
            &TYPE_START_SET,
            parse_positional_field,
            "a type",
        );
        context.stop_set.difference(&TYPE_STOP_SET);
        Ok(StructFields::Positional(list))
    } else {
        let fields = parse_comma_list(
            context,
            Tok::LBrace,
            Tok::RBrace,
            &TokenSet::from([Tok::Identifier, Tok::RestrictedIdentifier]),
            parse_field_annot,
            "a field",
        );
        Ok(StructFields::Named(fields))
    }
}

fn check_struct_visibility(visibility: Option<Visibility>, context: &mut Context) {
    let current_package = context.current_package;
    if let Some(Visibility::Public(loc)) = &visibility {
        context.check_feature(current_package, FeatureGate::StructTypeVisibility, *loc);
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
            if context.env.edition(current_package) == Edition::E2024_MIGRATION {
                context.add_diag(diag!(Migration::NeedsPublic, (loc, msg.clone())))
            } else {
                let mut err = diag!(Syntax::InvalidModifier, (loc, msg));
                err.add_note(note);
                context.add_diag(err);
            }
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
        context.add_diag(err);
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

// Parse a constant:
//      ConstantDecl = "const" <Identifier> ":" <Type> "=" <Exp> ";"
fn parse_constant_decl(
    doc: DocComment,
    attributes: Vec<Attributes>,
    start_loc: usize,
    modifiers: Modifiers,
    context: &mut Context,
) -> Result<Constant, Box<Diagnostic>> {
    let Modifiers {
        visibility,
        entry,
        native,
        macro_,
    } = modifiers;
    if let Some(vis) = visibility {
        let msg = "Invalid constant declaration. Constants cannot have visibility modifiers as \
                   they are always internal";
        context.add_diag(diag!(Syntax::InvalidModifier, (vis.loc().unwrap(), msg)));
    }
    check_no_modifier(context, NATIVE_MODIFIER, native, "constant");
    check_no_modifier(context, ENTRY_MODIFIER, entry, "constant");
    check_no_modifier(context, MACRO_MODIFIER, macro_, "constant");
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
        doc,
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
    doc: DocComment,
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<AddressDefinition, Box<Diagnostic>> {
    const UNEXPECTED_TOKEN: &str = "Invalid code unit. Expected 'address' or 'module'";
    let in_migration_mode =
        context.env.edition(context.current_package) == Edition::E2024_MIGRATION;

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
            if in_migration_mode {
                let loc = make_loc(
                    addr_name.loc.file_hash(),
                    addr_name.loc.start() as usize,
                    context.tokens.current_token_loc().end() as usize,
                );
                context.add_diag(diag!(Migration::AddressRemove, (loc, "address decl")));
            }
            context.tokens.advance()?;
            let mut modules = vec![];
            loop {
                let tok = context.tokens.peek();
                if tok == Tok::RBrace || tok == Tok::EOF {
                    break;
                }

                let mut attributes = parse_attributes(context)?;
                loop {
                    let doc = match_doc_comments(context);
                    let (module, next_mod_attributes) = parse_module(doc, attributes, context)?;

                    if in_migration_mode {
                        context.add_diag(diag!(
                            Migration::AddressAdd,
                            (
                                module.name.loc(),
                                format!("{}::", context.tokens.loc_contents(loc))
                            ),
                        ));
                    }

                    modules.push(module);
                    let Some(attrs) = next_mod_attributes else {
                        // no attributes returned from parse_module - just keep parsing next module
                        break;
                    };
                    // parse next module with the returned attributes
                    attributes = attrs;
                }
            }
            for module in &modules {
                if matches!(module.definition_mode, ModuleDefinitionMode::Semicolon) {
                    context.add_diag(diag!(
                        Declarations::InvalidModule,
                        (
                            module.name.loc(),
                            "Cannot define 'module' label in address block"
                        )
                    ));
                }
            }

            if in_migration_mode {
                let loc = context.tokens.current_token_loc();
                context.add_diag(diag!(Migration::AddressRemove, (loc, "close lbrace")));
            }

            consume_token(context.tokens, context.tokens.peek())?;
            modules
        }
        _ => return Err(unexpected_token_error(context.tokens, "'{'")),
    };

    if context.env.edition(context.current_package) != Edition::LEGACY && !in_migration_mode {
        let loc = addr_name.loc;
        let msg = "'address' blocks are deprecated. Use addresses \
                  directly in module definitions instead.";
        let mut diag = diag!(Editions::DeprecatedFeature, (loc, msg));
        for module in &modules {
            diag.add_secondary_label((
                module.name.loc(),
                format!("Replace with '{}::{}'", addr, module.name),
            ));
        }
        context.add_diag(diag);
    }

    check_no_doc_comment(context, loc, "'address' blocks", doc);
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
    let friend = parse_name_access_chain(
        context,
        /* macros */ false,
        /* tyargs */ false,
        || "a friend declaration",
    )?;
    if friend.value.is_macro().is_some() || friend.value.has_tyargs() {
        context.add_diag(diag!(
            Syntax::InvalidName,
            (friend.loc, "Invalid 'friend' name")
        ))
    }
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
    doc: DocComment,
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
        macro_,
    } = modifiers;
    check_no_modifier(context, NATIVE_MODIFIER, native, "use");
    check_no_modifier(context, ENTRY_MODIFIER, entry, "use");
    check_no_modifier(context, MACRO_MODIFIER, macro_, "use");
    let use_ = match context.tokens.peek() {
        Tok::Fun => {
            consume_token(context.tokens, Tok::Fun).unwrap();
            let function = parse_name_access_chain(
                context,
                /* macros */ false,
                /* tyargs */ false,
                || "a function name",
            )?;
            consume_token(context.tokens, Tok::As)?;
            let ty = parse_name_access_chain(
                context,
                /* macros */ false,
                /* tyargs */ false,
                || "a type name with no type arguments",
            )?;
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
                context.add_diag(diag!(Syntax::InvalidModifier, (vis.loc().unwrap(), msg)));
            }
            let address_start_loc = context.tokens.start_loc();
            let address = parse_leading_name_access(context)?;
            let colon_colon_loc = context.tokens.current_token_loc();
            if let Err(diag) = consume_token_(
                context.tokens,
                Tok::ColonColon,
                start_loc,
                " after an address in a use declaration",
            ) {
                context.add_diag(*diag);
                Use::Partial {
                    package: address,
                    colon_colon: None,
                    opening_brace: None,
                }
            } else {
                // add `;` to stop set to limit number of eaten tokens if the list is parsed
                // incorrectly
                context.stop_set.add(Tok::Semicolon);
                match context.tokens.peek() {
                    Tok::LBrace => {
                        let lbrace_loc = context.tokens.current_token_loc();
                        let parse_inner = |ctxt: &mut Context<'_, '_, '_>| {
                            parse_use_module(ctxt).map(|(name, _, use_)| (name, use_))
                        };
                        let use_decls = parse_comma_list(
                            context,
                            Tok::LBrace,
                            Tok::RBrace,
                            &TokenSet::from([Tok::Identifier]),
                            parse_inner,
                            "a module use clause",
                        );
                        let use_ = if use_decls.is_empty() {
                            // empty list does not make much sense as it contains no alias
                            // information and it actually helps IDE to treat this case as a partial
                            // use
                            Use::Partial {
                                package: address,
                                colon_colon: Some(colon_colon_loc),
                                opening_brace: Some(lbrace_loc),
                            }
                        } else {
                            Use::NestedModuleUses(address, use_decls)
                        };
                        context.stop_set.remove(Tok::Semicolon);
                        use_
                    }
                    _ => {
                        let use_ = match parse_use_module(context) {
                            Ok((name, end_loc, use_)) => {
                                let loc = make_loc(
                                    context.tokens.file_hash(),
                                    address_start_loc,
                                    end_loc,
                                );
                                let module_ident = sp(
                                    loc,
                                    ModuleIdent_ {
                                        address,
                                        module: name,
                                    },
                                );
                                Use::ModuleUse(module_ident, use_)
                            }
                            Err(diag) => {
                                context.add_diag(*diag);
                                Use::Partial {
                                    package: address,
                                    colon_colon: Some(colon_colon_loc),
                                    opening_brace: None,
                                }
                            }
                        };
                        context.stop_set.remove(Tok::Semicolon);
                        use_
                    }
                }
            }
        }
    };
    if let Err(diag) = consume_token(context.tokens, Tok::Semicolon) {
        context.add_diag(*diag);
    }
    let end_loc = context.tokens.previous_end_loc();
    let loc = make_loc(context.tokens.file_hash(), start_loc, end_loc);
    Ok(UseDecl {
        doc,
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
            let colon_colon_loc = context.tokens.current_token_loc();
            if let Err(diag) = consume_token(context.tokens, Tok::ColonColon) {
                context.add_diag(*diag);
                ModuleUse::Partial {
                    colon_colon: None,
                    opening_brace: None,
                }
            } else {
                match context.tokens.peek() {
                    Tok::LBrace => {
                        let lbrace_loc = context.tokens.current_token_loc();
                        let sub_uses = parse_comma_list(
                            context,
                            Tok::LBrace,
                            Tok::RBrace,
                            &TokenSet::from([Tok::Identifier]),
                            parse_use_member,
                            "a module member alias",
                        );
                        if sub_uses.is_empty() {
                            // empty list does not make much sense as it contains no alias
                            // information and it actually helps IDE to treat this case as a partial
                            // module use
                            ModuleUse::Partial {
                                colon_colon: Some(colon_colon_loc),
                                opening_brace: Some(lbrace_loc),
                            }
                        } else {
                            ModuleUse::Members(sub_uses)
                        }
                    }
                    _ => match parse_use_member(context) {
                        Ok(m) => ModuleUse::Members(vec![m]),
                        Err(diag) => {
                            context.add_diag(*diag);
                            ModuleUse::Partial {
                                colon_colon: Some(colon_colon_loc),
                                opening_brace: None,
                            }
                        }
                    },
                }
            }
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
//          |
//          <DocComments> ( "spec" | "module") (<LeadingNameAccess>::)?<ModuleName> ";"
//          ( <Attributes>
//              ( <FriendDecl> | <SpecBlock> |
//                <DocComments> <ModuleMemberModifiers>
//                    (<ConstantDecl> | <StructDecl> | <FunctionDecl> | <UseDecl>) )
//              )
//          )*
//
// Due to parsing error recovery, while parsing a module the parser may advance past the end of the
// current module and encounter the next module which also should be parsed. At the point of
// encountering this next module's starting keyword, its (optional) attributes are already parsed
// and should be used when constructing this next module - hence making them part of the returned
// result.
fn parse_module(
    doc: DocComment,
    attributes: Vec<Attributes>,
    context: &mut Context,
) -> Result<(ModuleDefinition, Option<Vec<Attributes>>), Box<Diagnostic>> {
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
    let definition_mode: ModuleDefinitionMode;
    match context.tokens.peek() {
        Tok::LBrace => {
            definition_mode = ModuleDefinitionMode::Braces;
            consume_token(context.tokens, Tok::LBrace)?;
        }
        Tok::Semicolon => {
            context.check_feature(
                context.current_package,
                FeatureGate::ModuleLabel,
                name.loc(),
            );
            definition_mode = ModuleDefinitionMode::Semicolon;
            consume_token(context.tokens, Tok::Semicolon)?;
        }
        _ => {
            return Err(unexpected_token_error(
                context.tokens,
                "'{' or ':' after the module name",
            ));
        }
    }

    let mut members = vec![];
    let mut next_mod_attributes = None;
    let mut stop_parsing = false;
    while context.tokens.peek() != Tok::RBrace {
        // If we are in semicolon mode, EOF is a fine place to stop.
        // If we see the `module` keyword, this is most-likely someone defining a second module
        // (erroneously), so we also bail in that case.
        if matches!(definition_mode, ModuleDefinitionMode::Semicolon)
            && (context.tokens.peek() == Tok::EOF || context.tokens.peek() == Tok::Module)
        {
            stop_parsing = true;
            break;
        }
        context.stop_set.union(&MODULE_MEMBER_OR_MODULE_START_SET);
        match parse_module_member(context) {
            Ok(m) => {
                context
                    .stop_set
                    .difference(&MODULE_MEMBER_OR_MODULE_START_SET);
                members.push(m);
            }
            Err(ErrCase::ContinueToModule(attrs)) => {
                context
                    .stop_set
                    .difference(&MODULE_MEMBER_OR_MODULE_START_SET);
                // while trying to parse module members, we moved past the current module and
                // encountered a new one - keep parsing it at a higher level, keeping the
                // already parsed attributes
                next_mod_attributes = Some(attrs);
                stop_parsing = true;
                break;
            }
            Err(ErrCase::Unknown(diag)) => {
                context
                    .stop_set
                    .difference(&MODULE_MEMBER_OR_MODULE_START_SET);
                context.add_diag(*diag);
                skip_to_next_desired_tok_or_eof(context, &MODULE_MEMBER_OR_MODULE_START_SET);
                if context.tokens.at(Tok::EOF) || context.tokens.at(Tok::Module) {
                    // either end of file or next module to potentially be parsed
                    stop_parsing = true;
                    break;
                }
            }
        }
    }
    if !stop_parsing {
        consume_token(context.tokens, Tok::RBrace)?;
    }
    let loc = make_loc(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
    );
    let def = ModuleDefinition {
        doc,
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members,
        definition_mode,
    };

    Ok((def, next_mod_attributes))
}

/// Skips tokens until reaching the desired one or EOF. Returns true if further parsing is
/// impossible and parser should stop.
fn skip_to_next_desired_tok_or_eof(context: &mut Context, desired_tokens: &TokenSet) {
    loop {
        if context.tokens.at(Tok::EOF) || context.tokens.at_set(desired_tokens) {
            break;
        }
        if let Err(diag) = context.tokens.advance() {
            // record diagnostics but keep advancing until encountering one of the desired tokens or
            // (which is eventually guaranteed) EOF
            context.add_diag(*diag);
        }
    }
}

/// Parse a single module member. Due to parsing error recovery, when attempting to parse the next
/// module member, the parser may have already advanced past the end of the current module and
/// encounter the next module which also should be parsed. While this is a member parsing error,
/// (optional) attributes for this presumed member (but in fact the next module) had already been
/// parsed and should be returned as part of the result to allow further parsing of the next module.
fn parse_module_member(context: &mut Context) -> Result<ModuleMember, ErrCase> {
    let attributes = parse_attributes(context)?;
    match context.tokens.peek() {
        // Top-level specification constructs
        Tok::Invariant => {
            let spec_string = consume_spec_string(context)?;
            consume_token(context.tokens, Tok::Semicolon)?;
            Ok(ModuleMember::Spec(spec_string))
        }
        Tok::Spec => {
            match context.tokens.lookahead() {
                Ok(Tok::Fun) | Ok(Tok::Native) => {
                    context.tokens.advance()?;
                    // Add an extra check for better error message
                    // if old syntax is used
                    if context.tokens.lookahead2() == Ok((Tok::Identifier, Tok::LBrace)) {
                        context.add_diag(*unexpected_token_error(
                            context.tokens,
                            "only 'spec', drop the 'fun' keyword",
                        ));
                    }
                    let spec_string = consume_spec_string(context)?;
                    Ok(ModuleMember::Spec(spec_string))
                }
                _ => {
                    // Regular spec block
                    let spec_string = consume_spec_string(context)?;
                    Ok(ModuleMember::Spec(spec_string))
                }
            }
        }
        // Regular move constructs
        Tok::Friend => Ok(ModuleMember::Friend(parse_friend_decl(
            attributes, context,
        )?)),
        _ => {
            let doc = match_doc_comments(context);
            let start_loc = context.tokens.start_loc();
            let modifiers = parse_module_member_modifiers(context)?;
            let tok = context.tokens.peek();
            match tok {
                Tok::Const => Ok(ModuleMember::Constant(parse_constant_decl(
                    doc, attributes, start_loc, modifiers, context,
                )?)),
                Tok::Fun => Ok(ModuleMember::Function(parse_function_decl(
                    doc, attributes, start_loc, modifiers, context,
                )?)),
                Tok::Struct => Ok(ModuleMember::Struct(parse_struct_decl(
                    doc, attributes, start_loc, modifiers, context,
                )?)),
                Tok::Enum => Ok(ModuleMember::Enum(parse_enum_decl(
                    doc, attributes, start_loc, modifiers, context,
                )?)),
                Tok::Use => Ok(ModuleMember::Use(parse_use_decl(
                    doc, attributes, start_loc, modifiers, context,
                )?)),
                _ => {
                    let diag = if matches!(context.tokens.peek(), Tok::Identifier)
                        && context.tokens.content() == "enum"
                        && !context
                            .env
                            .supports_feature(context.current_package, FeatureGate::Enums)
                    {
                        let msg = context
                            .env
                            .feature_edition_error_msg(FeatureGate::Enums, context.current_package)
                            .unwrap();
                        let mut diag = diag!(
                            Syntax::UnexpectedToken,
                            (context.tokens.current_token_loc(), msg)
                        );
                        diag.add_note(UPGRADE_NOTE);
                        Box::new(diag)
                    } else if context
                        .env
                        .supports_feature(context.current_package, FeatureGate::Move2024Keywords)
                    {
                        unexpected_token_error(
                            context.tokens,
                            &format!(
                                "a module member: {}",
                                format_oxford_list!(
                                    "or",
                                    "'{}'",
                                    [
                                        Tok::Spec,
                                        Tok::Use,
                                        Tok::Friend,
                                        Tok::Const,
                                        Tok::Fun,
                                        Tok::Struct,
                                        Tok::Enum
                                    ]
                                )
                            ),
                        )
                    } else {
                        unexpected_token_error(
                            context.tokens,
                            &format!(
                                "a module member: {}",
                                format_oxford_list!(
                                    "or",
                                    "'{}'",
                                    [
                                        Tok::Spec,
                                        Tok::Use,
                                        Tok::Friend,
                                        Tok::Const,
                                        Tok::Fun,
                                        Tok::Struct
                                    ]
                                )
                            ),
                        )
                    };
                    if tok == Tok::Module {
                        context.add_diag(*diag);
                        Err(ErrCase::ContinueToModule(attributes))
                    } else {
                        Err(ErrCase::Unknown(diag))
                    }
                }
            }
        }
    }
}

fn consume_spec_string(context: &mut Context) -> Result<Spanned<String>, Box<Diagnostic>> {
    let mut s = String::new();
    let start_loc = context.tokens.start_loc();
    // Fast-forward to the first left-brace.
    while !matches!(context.tokens.peek(), Tok::LBrace | Tok::EOF) {
        s.push_str(context.tokens.content());
        context.tokens.advance()?;
    }

    if context.tokens.peek() == Tok::EOF {
        return Err(unexpected_token_error(
            context.tokens,
            "a spec block: 'spec { ... }'",
        ));
    }

    s.push_str(context.tokens.content());
    context.tokens.advance()?;

    let mut count = 1;
    while count > 0 {
        let content = context.tokens.content();
        let tok = context.tokens.peek();
        s.push_str(content);
        if tok == Tok::LBrace {
            count += 1;
        } else if tok == Tok::RBrace {
            count -= 1;
        }
        context.tokens.advance()?;
    }

    let spanned = spanned(
        context.tokens.file_hash(),
        start_loc,
        context.tokens.previous_end_loc(),
        s,
    );
    Ok(spanned)
}

//**************************************************************************************************
// File
//**************************************************************************************************

// Parse a file:
//      File =
//          (<Attributes> (<AddressBlock> | <Module> ))*
fn parse_file(context: &mut Context) -> Vec<Definition> {
    let mut defs = vec![];
    while context.tokens.peek() != Tok::EOF {
        if let Err(diag) = parse_file_def(context, &mut defs) {
            context.add_diag(*diag);
            // skip to the next def and try parsing it if it's there (ignore address blocks as they
            // are pretty much defunct anyway)
            skip_to_next_desired_tok_or_eof(context, &TokenSet::from(&[Tok::Spec, Tok::Module]));
        }
    }
    defs
}

fn parse_file_def(
    context: &mut Context,
    defs: &mut Vec<Definition>,
) -> Result<(), Box<Diagnostic>> {
    let mut attributes = parse_attributes(context)?;
    match context.tokens.peek() {
        Tok::Spec | Tok::Module => {
            loop {
                let doc = match_doc_comments(context);
                let (module, next_mod_attributes) = parse_module(doc, attributes, context)?;
                if matches!(module.definition_mode, ModuleDefinitionMode::Semicolon) {
                    if let Some(prev) = defs.last() {
                        let msg =
                            "Cannot define a 'module' label form in a file with multiple modules";
                        let mut diag = diag!(Declarations::InvalidModule, (module.name.loc(), msg));
                        diag.add_secondary_label((prev.name_loc(), "Previous definition here"));
                        diag.add_note(
                            "Either move each 'module' label and definitions into its own file or \
                            define each as 'module <name> { contents }'",
                        );
                        context.add_diag(diag);
                    }
                }
                defs.push(Definition::Module(module));
                let Some(attrs) = next_mod_attributes else {
                    // no attributes returned from parse_module - just keep parsing next module
                    break;
                };
                // parse next module with the returned attributes
                attributes = attrs;
            }
        }
        _ => {
            let doc = match_doc_comments(context);
            defs.push(Definition::Address(parse_address_block(
                doc, attributes, context,
            )?))
        }
    }
    Ok(())
}

fn report_unmatched_doc_comments(context: &mut Context) {
    let unmatched = context.tokens.take_unmatched_doc_comments();
    let msg = "Documentation comment cannot be matched to a language item";
    let diags = unmatched
        .into_iter()
        .map(|(start, end, _)| {
            let loc = Loc::new(context.tokens.file_hash(), start, end);
            diag!(Syntax::InvalidDocComment, (loc, msg))
        })
        .collect();
    context
        .env
        .diagnostic_reporter_at_top_level()
        .add_diags(diags);
}

/// Parse the `input` string as a file of Move source code and return the
/// result as either a pair of FileDefinition and doc comments or some Diagnostics. The `file` name
/// is used to identify source locations in error messages.
pub fn parse_file_string(
    env: &CompilationEnv,
    file_hash: FileHash,
    input: &str,
    package: Option<Symbol>,
) -> Result<Vec<Definition>, Diagnostics> {
    let edition = env.edition(package);
    let mut tokens = Lexer::new(input, file_hash, edition);
    match tokens.advance() {
        Err(err) => Err(Diagnostics::from(vec![*err])),
        Ok(..) => Ok(()),
    }?;
    let context = &mut Context::new(env, &mut tokens, package);
    let result = parse_file(context);
    report_unmatched_doc_comments(context);
    Ok(result)
}
