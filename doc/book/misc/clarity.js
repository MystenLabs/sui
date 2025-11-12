// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Clarity syntax
 * Stacks/Bitcoin L2 smart contract language (Lisp-like)
 */

var module = module || {};

function hljsDefineClarity(hljs) {

    var KEYWORDS =
      'define-constant define-data-var define-fungible-token define-non-fungible-token ' +
      'define-map define-public define-private define-read-only define-trait ' +
      'impl-trait use-trait contract-call? let if begin asserts! unwrap! ' +
      'unwrap-err! unwrap-panic unwrap-err-panic match try! ok err some none ' +
      'is-eq is-some is-none is-ok is-err and or not';

    var BUILTINS =
      // Built-in functions
      'tx-sender contract-caller block-height stx-get-balance stx-transfer? ' +
      'stx-burn? ft-get-balance ft-transfer? ft-mint? ft-burn? nft-get-owner? ' +
      'nft-transfer? nft-mint? nft-burn? map-get? map-set map-insert map-delete ' +
      'var-get var-set print as-contract at-block get-block-info? ' +
      'sha256 sha512 sha512/256 keccak256 secp256k1-recover? secp256k1-verify ' +
      // Types
      'int uint bool principal buff string-ascii string-utf8 list tuple optional response';

      return {
      name: 'Clarity',
      aliases: ['clarity', 'clar'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'true false none',
        built_in: BUILTINS
      },
      illegal: /\S/,
      contains: [
        hljs.COMMENT(';;', '$'),
        {
          className: 'string',
          variants: [
            { begin: /"/, end: /"/ },
            { begin: /u"/, end: /"/ }
          ]
        },
        {
          className: 'number',
          variants: [
            { begin: '\\b0x([A-Fa-f0-9]+)' },
            { begin: '\\bu([0-9]+)' },
            { begin: '\\b([0-9]+)' },
          ],
          relevance: 0
        },
        {
          className: 'symbol',
          begin: /'[a-zA-Z_][a-zA-Z0-9_-]*/
        },
        {
          begin: '\\(', end: '\\)',
          contains: [
            'self',
            {
              className: 'name',
              begin: hljs.IDENT_RE,
              relevance: 0
            }
          ]
        },
        {
          className: 'literal',
          begin: /\b(true|false|none)\b/
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('clarity', hljsDefineClarity);
};

module.exports.definer = hljsDefineClarity;
