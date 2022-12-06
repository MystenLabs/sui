// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  any,
  boolean,
  Infer,
  literal,
  number,
  object,
  optional,
  record,
  string,
  union,
} from 'superstruct';
import { ObjectOwner } from './common';
import { TransactionDigest } from './common';
import {
  ObjectIdStruct,
  ObjectOwnerStruct,
  TransactionDigestStruct,
} from './shared';

export const ObjectTypeStruct = union([
  literal('moveObject'),
  literal('package'),
]);
export type ObjectType = Infer<typeof ObjectTypeStruct>;

export const SuiObjectRefStruct = object({
  /** Base64 string representing the object digest */
  digest: TransactionDigestStruct,
  /** Hex code as string representing the object id */
  objectId: string(),
  /** Object version */
  version: number(),
});
export type SuiObjectRef = Infer<typeof SuiObjectRefStruct>;

export const SuiObjectInfoStruct = object({
  ...SuiObjectRefStruct.schema,
  type: string(),
  owner: ObjectOwnerStruct,
  previousTransaction: TransactionDigestStruct,
});
export type SuiObjectInfo = Infer<typeof SuiObjectInfoStruct>;

export const ObjectContentFieldsStruct = record(string(), any());
export type ObjectContentFields = Infer<typeof ObjectContentFieldsStruct>;

export const MovePackageContentStruct = record(string(), string());
export type MovePackageContent = Infer<typeof MovePackageContentStruct>;

export const SuiMoveObjectStruct = object({
  /** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
  type: string(),
  /** Fields and values stored inside the Move object */
  fields: ObjectContentFieldsStruct,
  has_public_transfer: optional(boolean()),
});
export type SuiMoveObject = Infer<typeof SuiMoveObjectStruct>;

export const SuiMovePackageStruct = object({
  /** A mapping from module name to disassembled Move bytecode */
  disassembled: MovePackageContentStruct,
});
export type SuiMovePackage = Infer<typeof SuiMovePackageStruct>;

export const SuiData = union([
  object({
    dataType: ObjectTypeStruct,
    ...SuiMoveObjectStruct.schema,
  }),
  object({ dataType: ObjectTypeStruct, ...SuiMovePackageStruct.schema }),
]);

export const MIST_PER_SUI = BigInt(1000000000);

export type SuiMoveFunctionArgTypesResponse = SuiMoveFunctionArgType[];

export type SuiMoveFunctionArgType = string | { Object: string };

export type SuiMoveFunctionArgTypes = SuiMoveFunctionArgType[];

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

export const SuiObjectStruct = object({
  /** The meat of the object */
  data: SuiData,
  /** The owner of the object */
  owner: ObjectOwnerStruct,
  /** The digest of the transaction that created or last mutated this object */
  previousTransaction: TransactionDigestStruct,
  /**
   * The amount of SUI we would rebate if this object gets deleted.
   * This number is re-calculated each time the object is mutated based on
   * the present storage gas price.
   */
  storageRebate: number(),
  reference: SuiObjectRefStruct,
});
export type SuiObject = Infer<typeof SuiObjectStruct>;

export const ObjectStatusStruct = union([
  literal('Exists'),
  literal('NotExists'),
  literal('Deleted'),
]);
export type ObjectStatus = Infer<typeof ObjectStatusStruct>;

export type GetOwnedObjectsResponse = SuiObjectInfo[];

export const GetObjectDataResponseStruct = object({
  status: ObjectStatusStruct,
  details: union([SuiObjectStruct, ObjectIdStruct, SuiObjectRefStruct]),
});
export type GetObjectDataResponse = Infer<typeof GetObjectDataResponseStruct>;

export type ObjectDigest = string;
export type ObjectId = string;
export type SequenceNumber = number;
export type Order = 'ascending' | 'descending';

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* -------------------------- GetObjectDataResponse ------------------------- */

export function getObjectExistsResponse(
  resp: GetObjectDataResponse
): SuiObject | undefined {
  return resp.status !== 'Exists' ? undefined : (resp.details as SuiObject);
}

export function getObjectDeletedResponse(
  resp: GetObjectDataResponse
): SuiObjectRef | undefined {
  return resp.status !== 'Deleted' ? undefined : (resp.details as SuiObjectRef);
}

export function getObjectNotExistsResponse(
  resp: GetObjectDataResponse
): ObjectId | undefined {
  return resp.status !== 'NotExists' ? undefined : (resp.details as ObjectId);
}

export function getObjectReference(
  resp: GetObjectDataResponse
): SuiObjectRef | undefined {
  return (
    getObjectExistsResponse(resp)?.reference || getObjectDeletedResponse(resp)
  );
}

/* ------------------------------ SuiObjectRef ------------------------------ */

export function getObjectId(
  data: GetObjectDataResponse | SuiObjectRef
): ObjectId {
  if ('objectId' in data) {
    return data.objectId;
  }
  return (
    getObjectReference(data)?.objectId ?? getObjectNotExistsResponse(data)!
  );
}

export function getObjectVersion(
  data: GetObjectDataResponse | SuiObjectRef
): number | undefined {
  if ('version' in data) {
    return data.version;
  }
  return getObjectReference(data)?.version;
}

/* -------------------------------- SuiObject ------------------------------- */

export function getObjectType(
  resp: GetObjectDataResponse
): ObjectType | undefined {
  return getObjectExistsResponse(resp)?.data.dataType;
}

export function getObjectPreviousTransactionDigest(
  resp: GetObjectDataResponse
): TransactionDigest | undefined {
  return getObjectExistsResponse(resp)?.previousTransaction;
}

export function getObjectOwner(
  resp: GetObjectDataResponse
): ObjectOwner | undefined {
  return getObjectExistsResponse(resp)?.owner;
}

export function getSharedObjectInitialVersion(
  resp: GetObjectDataResponse
): number | undefined {
  const owner = getObjectOwner(resp);
  if (typeof owner === 'object' && 'Shared' in owner) {
    return owner.Shared.initial_shared_version;
  } else {
    return undefined;
  }
}

export function isSharedObject(resp: GetObjectDataResponse): boolean {
  const owner = getObjectOwner(resp);
  return typeof owner === 'object' && 'Shared' in owner;
}

export function isImmutableObject(resp: GetObjectDataResponse): boolean {
  const owner = getObjectOwner(resp);
  return owner === 'Immutable';
}

export function getMoveObjectType(
  resp: GetObjectDataResponse
): string | undefined {
  return getMoveObject(resp)?.type;
}

export function getObjectFields(
  resp: GetObjectDataResponse | SuiMoveObject
): ObjectContentFields | undefined {
  if ('fields' in resp) {
    return resp.fields;
  }
  return getMoveObject(resp)?.fields;
}

export function getMoveObject(
  data: GetObjectDataResponse | SuiObject
): SuiMoveObject | undefined {
  const suiObject = 'data' in data ? data : getObjectExistsResponse(data);
  if (suiObject?.data.dataType !== 'moveObject') {
    return undefined;
  }
  return suiObject.data as SuiMoveObject;
}

export function hasPublicTransfer(
  data: GetObjectDataResponse | SuiObject
): boolean {
  return getMoveObject(data)?.has_public_transfer ?? false;
}

export function getMovePackageContent(
  data: GetObjectDataResponse | SuiMovePackage
): MovePackageContent | undefined {
  if ('disassembled' in data) {
    return data.disassembled;
  }
  const suiObject = getObjectExistsResponse(data);
  if (suiObject?.data.dataType !== 'package') {
    return undefined;
  }
  return (suiObject.data as SuiMovePackage).disassembled;
}

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
