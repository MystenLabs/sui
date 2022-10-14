// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, TypeTag } from '../../types';

const VECTOR_REGEX = /^vector<(.+)>$/;
const STRUCT_REGEX = /^([^:]+)::([^:]+)::(.+)/;
const STRUCT_TYPE_TAG_REGEX = /^[^<]+<(.+)>$/;

export class TypeTagSerializer {
  parseFromStr(str: string): TypeTag {
    if (str === 'address') {
      return { address: null };
    } else if (str === 'bool') {
      return { bool: null };
    } else if (str === 'u8') {
      return { u8: null };
    } else if (str === 'u64') {
      return { u64: null };
    } else if (str === 'signer') {
      return { signer: null };
    }
    const vectorMatch = str.match(VECTOR_REGEX);
    if (vectorMatch) {
      return { vector: this.parseFromStr(vectorMatch[1]) };
    }

    const structMatch = str.match(STRUCT_REGEX);
    if (structMatch) {
      try {
        return {
          struct: {
            address: normalizeSuiAddress(structMatch[1]),
            module: structMatch[2],
            name: structMatch[3].match(/^([^<]+)/)![1],
            typeParams: this.parseStructTypeTag(structMatch[3]),
          },
        };
      } catch (e) {
        throw new Error(`Encounter error parsing type args for ${str}`);
      }
    }

    throw new Error(
      `Encounter unexpected token when parsing type args for ${str}`
    );
  }

  parseStructTypeTag(str: string): TypeTag[] {
    const typeTagsMatch = str.match(STRUCT_TYPE_TAG_REGEX);
    if (!typeTagsMatch) {
      return [];
    }
    // TODO: This will fail if the struct has nested type args with commas. Need
    // to implement proper parsing for this case
    const typeTags = typeTagsMatch[1].split(',');
    return typeTags.map((tag) => this.parseFromStr(tag));
  }
}
