// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Motoko syntax
 * DFINITY Internet Computer smart contract language
 */

var module = module || {};

function hljsDefineMotoko(hljs) {

    var KEYWORDS =
      'actor async await assert break case catch class continue debug debug_show ' +
      'do else flexible for func if ignore import in module not null object or ' +
      'label let loop private public query return shared stable switch throw to_candid ' +
      'try type var while with composite system';

    var BUILTINS =
      // Built-in types
      'Nat Nat8 Nat16 Nat32 Nat64 Int Int8 Int16 Int32 Int64 Float Bool Text Char ' +
      'Blob Null Principal Error Array Buffer Hash List Trie TrieMap HashMap ' +
      'Option Result Iter Any None ' +
      // Built-in functions
      'abs ignore debug_show Array_tabulate Array_init';

      return {
      name: 'Motoko',
      aliases: ['motoko', 'mo'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'true false null',
        built_in: BUILTINS
      },
      illegal: '</',
      contains: [
        hljs.C_LINE_COMMENT_MODE,
        hljs.COMMENT('/\\*', '\\*/', {contains: ['self']}),
        hljs.QUOTE_STRING_MODE,
        {
          className: 'string',
          begin: /#"/, end: /"/
        },
        {
          className: 'number',
          variants: [
            { begin: '\\b0x([A-Fa-f0-9_]+)' },
            { begin: '\\b([0-9][0-9_]*(\\.[0-9_]+)?([eE][+-]?[0-9_]+)?)' },
          ],
          relevance: 0
        },
        {
          className: 'function',
          beginKeywords: 'func', end: '(\\(|<|=)', excludeEnd: true,
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          className: 'class',
          beginKeywords: 'actor class object type module', end: '{',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ],
          illegal: '[\\w\\d]'
        },
        {
          className: 'meta',
          begin: /@/, end: /$/
        },
        {
          className: 'keyword',
          begin: /#[a-zA-Z_][a-zA-Z0-9_]*/
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('motoko', hljsDefineMotoko);
};

module.exports.definer = hljsDefineMotoko;
