// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { array, Infer, object, string, union } from 'superstruct';

export type SuiMoveFunctionArgTypesResponse = Infer<
  typeof SuiMoveFunctionArgType
>[];

export const SuiMoveFunctionArgType = union([
  string(),
  object({ Object: string() }),
]);

export const SuiMoveFunctionArgTypes = array(SuiMoveFunctionArgType);
export type SuiMoveFunctionArgTypes = Infer<typeof SuiMoveFunctionArgTypes>;

export type SuiMoveNormalizedModules = Record<string, SuiMoveNormalizedModule>;

export type SuiMoveNormalizedModule = {
  file_format_version: number;
  address: string;
  name: string;
  friends: SuiMoveModuleId[];
  structs: Record<string, SuiMoveNormalizedStruct>;
  exposed_functions: Record<string, SuiMoveNormalizedFunction>;
};

export type SuiMoveModuleId = {
  address: string;
  name: string;
};

export type SuiMoveNormalizedStruct = {
  abilities: SuiMoveAbilitySet;
  type_parameters: SuiMoveStructTypeParameter[];
  fields: SuiMoveNormalizedField[];
};

export type SuiMoveStructTypeParameter = {
  constraints: SuiMoveAbilitySet;
  is_phantom: boolean;
};

export type SuiMoveNormalizedField = {
  name: string;
  type_: SuiMoveNormalizedType;
};

export type SuiMoveNormalizedFunction = {
  visibility: SuiMoveVisibility;
  is_entry: boolean;
  type_parameters: SuiMoveAbilitySet[];
  parameters: SuiMoveNormalizedType[];
  return_: SuiMoveNormalizedType[];
};

export type SuiMoveVisibility = 'Private' | 'Public' | 'Friend';

export type SuiMoveTypeParameterIndex = number;

export type SuiMoveAbilitySet = {
  abilities: string[];
};

export type SuiMoveNormalizedType =
  | string
  | SuiMoveNormalizedTypeParameterType
  | { Reference: SuiMoveNormalizedType }
  | { MutableReference: SuiMoveNormalizedType }
  | { Vector: SuiMoveNormalizedType }
  | SuiMoveNormalizedStructType;

export type SuiMoveNormalizedTypeParameterType = {
  TypeParameter: SuiMoveTypeParameterIndex;
};

export type SuiMoveNormalizedStructType = {
  Struct: {
    address: string;
    module: string;
    name: string;
    type_arguments: SuiMoveNormalizedType[];
  };
};

export function extractMutableReference(
  normalizedType: SuiMoveNormalizedType
): SuiMoveNormalizedType | undefined {
  return typeof normalizedType === 'object' &&
    'MutableReference' in normalizedType
    ? normalizedType.MutableReference
    : undefined;
}

export function extractReference(
  normalizedType: SuiMoveNormalizedType
): SuiMoveNormalizedType | undefined {
  return typeof normalizedType === 'object' && 'Reference' in normalizedType
    ? normalizedType.Reference
    : undefined;
}

export function extractStructTag(
  normalizedType: SuiMoveNormalizedType
): SuiMoveNormalizedStructType | undefined {
  if (typeof normalizedType === 'object' && 'Struct' in normalizedType) {
    return normalizedType;
  }

  const ref = extractReference(normalizedType);
  const mutRef = extractMutableReference(normalizedType);

  if (typeof ref === 'object' && 'Struct' in ref) {
    return ref;
  }

  if (typeof mutRef === 'object' && 'Struct' in mutRef) {
    return mutRef;
  }
  return undefined;
}
