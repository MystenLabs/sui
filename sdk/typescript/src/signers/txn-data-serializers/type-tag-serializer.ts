// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TypeTag } from '../../types';

const VECTOR_REGEX = /^vector<(.+)>$/;
const STRUCT_REGEX = /^([^:]+)::([^:]+)::([^<]+)(<(.+)>)?/;

export class TypeTagSerializer {
  parseFromStr(str: string): TypeTag {
    if (str === 'address') {
      return { address: null };
    } else if (str === 'bool') {
      return { bool: null };
    } else if (str === 'u8') {
      return { u8: null };
    } else if (str === 'u16') {
      return { u16: null };
    } else if (str === 'u32') {
      return { u32: null };
    } else if (str === 'u64') {
      return { u64: null };
    } else if (str === 'u128') {
      return { u128: null };
    } else if (str === 'u256') {
      return { u256: null };
    } else if (str === 'signer') {
      return { signer: null };
    }
    const vectorMatch = str.match(VECTOR_REGEX);
    if (vectorMatch) {
      return { vector: this.parseFromStr(vectorMatch[1]) };
    }

    const structMatch = str.match(STRUCT_REGEX);
    if (structMatch) {
      return {
        struct: {
          address: structMatch[1],
          module: structMatch[2],
          name: structMatch[3],
          typeParams:
            structMatch[5] === undefined
              ? []
              : this.parseStructTypeArgs(structMatch[5]),
        },
      };
    }

    throw new Error(
      `Encountered unexpected token when parsing type args for ${str}`
    );
  }

  parseStructTypeArgs(str: string): TypeTag[] {
    // split `str` by all `,` outside angle brackets
    const tok: Array<string> = [];
    let word = '';
    let nestedAngleBrackets = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str[i];
      if (char === '<') {
        nestedAngleBrackets++;
      }
      if (char === '>') {
        nestedAngleBrackets--;
      }
      if (nestedAngleBrackets === 0 && char === ',') {
        tok.push(word.trim());
        word = '';
        continue;
      }
      word += char;
    }

    tok.push(word.trim());

    return tok.map((tok) => this.parseFromStr(tok));
  }
}
