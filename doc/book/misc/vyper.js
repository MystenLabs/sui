// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Vyper syntax
 * Python-style smart contract language for Ethereum
 */

var module = module || {};

function hljsDefineVyper(hljs) {

    var KEYWORDS =
      'def event struct interface implements contract log indexed nonreentrant ' +
      'payable public internal private external view pure constant immutable ' +
      'if elif else for while pass break continue return assert raise in not and or ' +
      'import from as True False None';

    var BUILTINS =
      // Built-in functions
      'send raw_call create_forwarder_to selfdestruct keccak256 sha256 ecrecover ' +
      'ecadd ecmul ecpairing blockhash bitwise_not bitwise_and bitwise_or bitwise_xor ' +
      'uint2str concat slice len method_id empty clear ' +
      // Types
      'int128 int256 uint8 uint256 decimal bool bytes32 address String Bytes ' +
      'DynArray HashMap';

      return {
      name: 'Vyper',
      aliases: ['vyper', 'vy'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'True False None',
        built_in: BUILTINS
      },
      illegal: '</',
      contains: [
        hljs.HASH_COMMENT_MODE,
        hljs.inherit(hljs.QUOTE_STRING_MODE, {illegal: null}),
        hljs.inherit(hljs.APOS_STRING_MODE, {illegal: null}),
        {
          className: 'string',
          begin: /"""/, end: /"""/,
          relevance: 10
        },
        {
          className: 'number',
          variants: [
            { begin: '\\b0x([A-Fa-f0-9]+)' },
            { begin: '\\b([0-9]+)' },
          ],
          relevance: 0
        },
        {
          className: 'function',
          beginKeywords: 'def', end: ':', excludeEnd: true,
          contains: [
            hljs.UNDERSCORE_TITLE_MODE,
            {
              className: 'params',
              begin: /\(/, end: /\)/,
              contains: ['self', hljs.HASH_COMMENT_MODE]
            }
          ]
        },
        {
          className: 'class',
          beginKeywords: 'struct interface event', end: ':',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ]
        },
        {
          className: 'meta',
          begin: /@/, end: /$/,
          contains: [hljs.HASH_COMMENT_MODE]
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('vyper', hljsDefineVyper);
};

module.exports.definer = hljsDefineVyper;
