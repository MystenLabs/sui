// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  Infer,
  object,
  string,
  union,
  boolean,
  define,
  number,
  literal,
  record,
  is,
} from 'superstruct';

export type SuiMoveFunctionArgTypesResponse = Infer<
  typeof SuiMoveFunctionArgType
>[];

export const SuiMoveFunctionArgType = union([
  string(),
  object({ Object: string() }),
]);

export const SuiMoveFunctionArgTypes = array(SuiMoveFunctionArgType);
export type SuiMoveFunctionArgTypes = Infer<typeof SuiMoveFunctionArgTypes>;

export const SuiMoveModuleId = object({
  address: string(),
  name: string(),
});
export type SuiMoveModuleId = Infer<typeof SuiMoveModuleId>;

export const SuiMoveVisibility = union([
  literal('Private'),
  literal('Public'),
  literal('Friend'),
]);
export type SuiMoveVisibility = Infer<typeof SuiMoveVisibility>;

export const SuiMoveAbilitySet = object({
  abilities: array(string()),
});
export type SuiMoveAbilitySet = Infer<typeof SuiMoveAbilitySet>;

export const SuiMoveStructTypeParameter = object({
  constraints: SuiMoveAbilitySet,
  isPhantom: boolean(),
});
export type SuiMoveStructTypeParameter = Infer<
  typeof SuiMoveStructTypeParameter
>;

export const SuiMoveNormalizedTypeParameterType = object({
  TypeParameter: number(),
});
export type SuiMoveNormalizedTypeParameterType = Infer<
  typeof SuiMoveNormalizedTypeParameterType
>;

// export type SuiMoveNormalizedType =
//   | { type: 'Bool' }
//   | { type: 'U8' }
//   | { type: 'U16' }
//   | { type: 'U32' }
//   | { type: 'U64' }
//   | { type: 'U128' }
//   | { type: 'U256' }
//   | { type: 'Address' }
//   | { type: 'Signer' }
//   | {
//       type: 'Struct';
//       content: {
//         address: string;
//         module: string;
//         name: string;
//         typeArguments: SuiMoveNormalizedType[];
//       };
//     }
//   | { type: 'Vector'; content: SuiMoveNormalizedType }
//   | { type: 'TypeParameter'; content: number }
//   | { type: 'Reference'; content: SuiMoveNormalizedType }
//   | { type: 'MutableReference'; content: SuiMoveNormalizedType };

export type SuiMoveNormalizedType = { type: string; content?: any };

function isSuiMoveNormalizedType(
  value: unknown,
): value is SuiMoveNormalizedType {
  if (!value) return false;
  const valueProperties = value as { type: string; content: unknown };

  if (
    [
      'Bool',
      'U8',
      'U16',
      'U32',
      'U64',
      'U128',
      'U256',
      'Address',
      'Signer',
    ].includes(valueProperties.type)
  )
    return true;
  if (
    ['Vector', 'Reference', 'MutableReference'].includes(
      valueProperties.type,
    ) &&
    is(valueProperties.content, SuiMoveNormalizedType)
  )
    return true;
  if (
    valueProperties.type === 'TypeParameter' &&
    typeof valueProperties.content === 'number'
  )
    return true;
  if (valueProperties.type === 'Struct') {
    const contents = valueProperties.content as Record<string, unknown>;
    return (
      typeof contents.address === 'string' &&
      typeof contents.module === 'string' &&
      typeof name === 'string' &&
      Array.isArray(contents.typeArguments) &&
      contents.typeArguments.every((value) => is(value, SuiMoveNormalizedType))
    );
  }
  return false;
}

export const SuiMoveNormalizedType = define<SuiMoveNormalizedType>(
  'SuiMoveNormalizedType',
  isSuiMoveNormalizedType,
);

export type SuiMoveNormalizedStructType = {
  Struct: {
    address: string;
    module: string;
    name: string;
    typeArguments: SuiMoveNormalizedType[];
  };
};

function isSuiMoveNormalizedStructType(
  value: unknown,
): value is SuiMoveNormalizedStructType {
  if (!value || typeof value !== 'object') return false;

  const valueProperties = value as Record<string, unknown>;
  if (!valueProperties.Struct || typeof valueProperties.Struct !== 'object')
    return false;

  const structProperties = valueProperties.Struct as Record<string, unknown>;
  if (
    typeof structProperties.address !== 'string' ||
    typeof structProperties.module !== 'string' ||
    typeof structProperties.name !== 'string' ||
    !Array.isArray(structProperties.typeArguments) ||
    !structProperties.typeArguments.every((value) =>
      isSuiMoveNormalizedType(value),
    )
  ) {
    return false;
  }

  return true;
}

// NOTE: This type is recursive, so we need to manually implement it:
export const SuiMoveNormalizedStructType = define<SuiMoveNormalizedStructType>(
  'SuiMoveNormalizedStructType',
  isSuiMoveNormalizedStructType,
);

export const SuiMoveNormalizedFunction = object({
  visibility: SuiMoveVisibility,
  isEntry: boolean(),
  typeParameters: array(SuiMoveAbilitySet),
  parameters: array(SuiMoveNormalizedType),
  return: array(SuiMoveNormalizedType),
});
export type SuiMoveNormalizedFunction = Infer<typeof SuiMoveNormalizedFunction>;

export const SuiMoveNormalizedField = object({
  name: string(),
  type: SuiMoveNormalizedType,
});
export type SuiMoveNormalizedField = Infer<typeof SuiMoveNormalizedField>;

export const SuiMoveNormalizedStruct = object({
  abilities: SuiMoveAbilitySet,
  typeParameters: array(SuiMoveStructTypeParameter),
  fields: array(SuiMoveNormalizedField),
});
export type SuiMoveNormalizedStruct = Infer<typeof SuiMoveNormalizedStruct>;

export const SuiMoveNormalizedModule = object({
  fileFormatVersion: number(),
  address: string(),
  name: string(),
  friends: array(SuiMoveModuleId),
  structs: record(string(), SuiMoveNormalizedStruct),
  exposedFunctions: record(string(), SuiMoveNormalizedFunction),
});
export type SuiMoveNormalizedModule = Infer<typeof SuiMoveNormalizedModule>;

export const SuiMoveNormalizedModules = record(
  string(),
  SuiMoveNormalizedModule,
);
export type SuiMoveNormalizedModules = Infer<typeof SuiMoveNormalizedModules>;

export function extractMutableReference(
  normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
  return typeof normalizedType === 'object' &&
    'MutableReference' === normalizedType.type &&
    typeof normalizedType.content === 'object'
    ? normalizedType.content
    : undefined;
}

export function extractReference(
  normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
  return typeof normalizedType === 'object' &&
    'Reference' === normalizedType.type
    ? normalizedType.content
    : undefined;
}

export function extractStructTag(
  normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedStructType | undefined {
  if (typeof normalizedType === 'object' && 'Struct' === normalizedType.type) {
    return { Struct: normalizedType.content };
  }

  const ref = extractReference(normalizedType);
  const mutRef = extractMutableReference(normalizedType);

  if (typeof ref === 'object' && ref.type === 'Struct') {
    return { Struct: ref.content };
  }

  if (typeof mutRef === 'object' && mutRef.type === 'Struct') {
    return { Struct: mutRef.content };
  }
  return undefined;
}
