// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isObjectContent } from '../../index.guard';
import {
  getObjectContent,
  GetObjectInfoResponse,
  ObjectContent,
  ObjectContentField,
  ObjectContentFields,
  ObjectExistsInfo,
} from '../objects';

/**
 * Simplifies the common Move Object Content. This will be implemented
 * in the Gateway server level after DevNet.
 */
export function transformGetObjectInfoResponse(resp: GetObjectInfoResponse) {
  const content = getObjectContent(resp);
  if (content != null) {
    (resp.details as ObjectExistsInfo).object.contents = transformObjectContent(
      content
    );
  }
  return resp;
}

export function transformObjectContent(input: ObjectContent): ObjectContent {
  let fields: ObjectContentFields = {};
  Object.entries<any>(input.fields).forEach(([key, value]) => {
    if (!isObjectContent(value)) {
      fields[key] = value;
      return;
    }
    const parsers: typeof MoveObjectContentTransformer[] = [
      BalanceTransformer,
      StringTransformer,
      UniqueIDTransformer,
    ];
    let isTransformed = false;
    for (let p of parsers) {
      if (p.canTransform(value)) {
        fields[key] = p.toFieldValue(value);
        isTransformed = true;
        break;
      }
    }
    if (!isTransformed) {
      fields[key] = transformObjectContent(value);
    }
  });
  return {
    fields,
    type: input.type,
  };
}

abstract class MoveObjectContentTransformer {
  static toFieldValue(_input: ObjectContent): ObjectContentField {
    throw new Error('Children classes must override');
  }

  static canTransform(_input: ObjectContent): boolean {
    throw new Error('Children classes must override');
  }
}

class StringTransformer extends MoveObjectContentTransformer {
  static toFieldValue(input: ObjectContent): ObjectContentField {
    const bytes = input.fields['bytes'] as number[];
    switch (input.type) {
      case '0x1::ASCII::String':
        return bytes.map(n => String.fromCharCode(n)).join('');
      case '0x2::UTF8::String':
        return stringFromUTF8Array(new Uint8Array(bytes))!;
    }
    return input;
  }

  static canTransform(input: ObjectContent): boolean {
    return (
      input.type === '0x2::UTF8::String' || input.type === '0x1::ASCII::String'
    );
  }
}

class UniqueIDTransformer extends MoveObjectContentTransformer {
  static toFieldValue(input: ObjectContent): ObjectContentField {
    if (UniqueIDTransformer.canTransform(input)) {
      return (input.fields['id'] as ObjectContent).fields['bytes'];
    }
    return input;
  }

  static canTransform(input: ObjectContent): boolean {
    return (
      input.type === '0x2::ID::UniqueID' &&
      isObjectContent(input.fields['id']) &&
      input.fields['id'].type === '0x2::ID::ID'
    );
  }
}

class BalanceTransformer extends MoveObjectContentTransformer {
  static toFieldValue(input: ObjectContent): ObjectContentField {
    if (BalanceTransformer.canTransform(input)) {
      return input.fields['value'] as number;
    }
    return input;
  }

  static canTransform(input: ObjectContent): boolean {
    return input.type.startsWith('0x2::Balance::Balance');
  }
}

// from https://weblog.rogueamoeba.com/2017/02/27/javascript-correctly-converting-a-byte-array-to-a-utf-8-string/
function stringFromUTF8Array(data: Uint8Array): string | null {
  const extraByteMap = [1, 1, 1, 1, 2, 2, 3, 0];
  var count = data.length;
  var str = '';

  for (var index = 0; index < count; ) {
    var ch = data[index++];
    if (ch & 0x80) {
      var extra = extraByteMap[(ch >> 3) & 0x07];
      if (!(ch & 0x40) || !extra || index + extra > count) return null;

      ch = ch & (0x3f >> extra);
      for (; extra > 0; extra -= 1) {
        var chx = data[index++];
        if ((chx & 0xc0) != 0x80) return null;

        ch = (ch << 6) | (chx & 0x3f);
      }
    }

    str += String.fromCharCode(ch);
  }

  return str;
}
