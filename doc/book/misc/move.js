// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Move syntax
 * Use this file to make custom HighlightJS build
 */

var module = module || {};

function hljsDefineMove(hljs) {

    var NUM_SUFFIX = '(u(8|64|128))\?';
    var KEYWORDS =
      'abort acquires as break continue copy copyable define else false fun ' +
      'if invariant let loop module move native public resource return spec ' +
      'struct true use while';
    var BUILTINS =
      // functions
      'move_to_sender borrow_global emit_event borrow_global_mut ' +
      'move_from exists ' +
      // types
      'u8 u64 u128 ' +
      'vector address bool';

      return {
      name: 'Move',
      aliases: ['move'],
      keywords: {
        keyword:
          KEYWORDS,
        literal:
          'true false',
        built_in:
          BUILTINS
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
            { begin: '\\b0x([A-Fa-f0-9_]+)' },
            { begin: '\\b([0-9]+)' + NUM_SUFFIX },
          ],
          relevance: 0
        },
        {
          className: 'function',
          beginKeywords: 'fun', end: '(\\(|<)', excludeEnd: true,
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          className: 'class',
          beginKeywords: 'struct resource module', end: '{',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ],
          illegal: '[\\w\\d]'
        },
        {
          begin: hljs.IDENT_RE + '::',
          keywords: {built_in: BUILTINS}
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('move', hljsDefineMove);
};

module.exports.definer = hljsDefineMove;
