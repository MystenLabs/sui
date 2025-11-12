// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Ink! syntax
 * Polkadot/Substrate smart contract language (Rust-based)
 */

var module = module || {};

function hljsDefineInk(hljs) {

    var NUM_SUFFIX = '(u(8|16|32|64|128)|i(8|16|32|64|128)|f(32|64))\?';
    var KEYWORDS =
      'abstract as async await become box break const continue crate do dyn ' +
      'else enum extern false final fn for if impl in let loop macro match mod ' +
      'move mut override priv pub ref return self Self static struct super trait ' +
      'true try type typeof unsafe unsized use virtual where while yield';

    var BUILTINS =
      // ink! specific types and macros
      'AccountId Balance Hash Timestamp BlockNumber String Vec Option Result ' +
      'contract storage event topic message payable constructor selector ' +
      'CallBuilder CreateBuilder ink_e2e ink_lang ink_storage ink_prelude ' +
      'ink_primitives spread_allocate env EmitEvent';

      return {
      name: 'Ink',
      aliases: ['ink'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'true false Some None Ok Err',
        built_in: BUILTINS
      },
      lexemes: hljs.IDENT_RE + '!?',
      illegal: '</',
      contains: [
        hljs.C_LINE_COMMENT_MODE,
        hljs.COMMENT('/\\*', '\\*/', {contains: ['self']}),
        hljs.inherit(hljs.QUOTE_STRING_MODE, {begin: /b?"/, illegal: null}),
        {
          className: 'string',
          variants: [
             { begin: /r(#*)"(.|\n)*?"\1(?!#)/ },
             { begin: /b?'\\?(x\w{2}|u\w{4}|U\w{8}|.)'/ }
          ]
        },
        {
          className: 'symbol',
          begin: /'[a-zA-Z_][a-zA-Z0-9_]*/
        },
        {
          className: 'number',
          variants: [
            { begin: '\\b0x([A-Fa-f0-9_]+)' + NUM_SUFFIX },
            { begin: '\\b0o([0-7_]+)' + NUM_SUFFIX },
            { begin: '\\b0b([01_]+)' + NUM_SUFFIX },
            { begin: '\\b([0-9][0-9_]*(\\.[0-9_]+)?)' + NUM_SUFFIX },
          ],
          relevance: 0
        },
        {
          className: 'function',
          beginKeywords: 'fn', end: '(\\(|<)', excludeEnd: true,
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          className: 'class',
          beginKeywords: 'struct enum union trait impl', end: '{',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ],
          illegal: '[\\w\\d]'
        },
        {
          className: 'meta',
          begin: '#\\!?\\[', end: '\\]',
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          begin: hljs.IDENT_RE + '::',
          keywords: {built_in: BUILTINS}
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('ink', hljsDefineInk);
};

module.exports.definer = hljsDefineInk;
