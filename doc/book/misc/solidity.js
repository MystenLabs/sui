// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * HighlightJS Solidity syntax
 * Ethereum smart contract language
 */

var module = module || {};

function hljsDefineSolidity(hljs) {

    var KEYWORDS =
      'abstract after alias apply auto case catch copyof default define final ' +
      'immutable implements in inline let macro match mutable null of override ' +
      'partial promise reference relocatable sealed sizeof static supports switch ' +
      'typedef unchecked pragma solidity contract library interface function ' +
      'modifier constructor event struct enum mapping address bool string var ' +
      'bytes byte int uint fixed ufixed memory storage calldata public private ' +
      'internal external pure view payable constant anonymous indexed assembly ' +
      'return emit revert require assert if else for while do break continue ' +
      'throw try catch new delete this super true false is as using type virtual';

    var BUILTINS =
      // Global functions
      'keccak256 sha256 ripemd160 ecrecover addmod mulmod selfdestruct suicide ' +
      'blockhash gasleft abi block msg tx now ' +
      // Types
      'address bool string bytes byte int uint int8 int16 int32 int64 int128 int256 ' +
      'uint8 uint16 uint32 uint64 uint128 uint256 byte bytes1 bytes2 bytes4 bytes8 ' +
      'bytes16 bytes32';

      return {
      name: 'Solidity',
      aliases: ['solidity', 'sol'],
      keywords: {
        keyword: KEYWORDS,
        literal: 'true false wei szabo finney ether',
        built_in: BUILTINS
      },
      lexemes: hljs.IDENT_RE + '!?',
      illegal: '</',
      contains: [
        hljs.C_LINE_COMMENT_MODE,
        hljs.COMMENT('/\\*', '\\*/', {contains: ['self']}),
        hljs.inherit(hljs.QUOTE_STRING_MODE, {begin: /"/, illegal: null}),
        hljs.inherit(hljs.APOS_STRING_MODE, {illegal: null}),
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
          beginKeywords: 'function modifier constructor', end: '(\\(|<)', excludeEnd: true,
          contains: [hljs.UNDERSCORE_TITLE_MODE]
        },
        {
          className: 'class',
          beginKeywords: 'contract library interface struct enum', end: '{',
          contains: [
            hljs.inherit(hljs.UNDERSCORE_TITLE_MODE, {endsParent: true})
          ],
          illegal: '[\\w\\d]'
        },
        {
          beginKeywords: 'pragma solidity'
        }
      ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('solidity', hljsDefineSolidity);
};

module.exports.definer = hljsDefineSolidity;
