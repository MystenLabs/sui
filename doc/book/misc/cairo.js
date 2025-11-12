// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Cairo syntax
 * StarkNet smart contract language
 */

var module = module || {};

function hljsDefineCairo(hljs) {

    var KEYWORDS =
      'func let const if else end assert return struct namespace from import ' +
      'as alloc new with tempvar ap cast nondet dw felt codeoffset local ' +
      'extern view external constructor storage_var event l1_handler interface ' +
      'mod trait impl of match loop break continue';

    var BUILTINS =
      // Built-in functions and types
      'felt u8 u16 u32 u64 u128 u256 bool ContractAddress ClassHash StorageAddress ' +
      'Span Array Option Result Box ' +
      'get_caller_address get_contract_address get_block_timestamp get_block_number ' +
      'storage_read storage_write emit_event call_contract deploy';

      return {
      name: 'Cairo',
      aliases: ['cairo'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'true false',
        built_in: BUILTINS
      },
      lexemes: hljs.IDENT_RE + '!?',
      illegal: '</',
      contains: [
        hljs.HASH_COMMENT_MODE,
        hljs.C_LINE_COMMENT_MODE,
        {
          className: 'string',
          variants: [
            hljs.QUOTE_STRING_MODE,
            { begin: /'[a-zA-Z_][a-zA-Z0-9_]*/ }
          ]
        },
        {
          className: 'number',
          variants: [
            { begin: '\\b0x([A-Fa-f0-9_]+)' },
            { begin: '\\b([0-9]+)' },
          ],
          relevance: 0
        },
        {
          className: 'function',
          beginKeywords: 'func fn', end: '(\\(|<|->)', excludeEnd: true,
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          className: 'class',
          beginKeywords: 'struct trait impl namespace contract interface', end: '{',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ],
          illegal: '[\\w\\d]'
        },
        {
          className: 'meta',
          begin: '#\\[', end: '\\]',
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('cairo', hljsDefineCairo);
};

module.exports.definer = hljsDefineCairo;
