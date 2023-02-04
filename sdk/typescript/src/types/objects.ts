// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  any,
  array,
  assign,
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
import { ObjectId, ObjectOwner, TransactionDigest } from './common';

export const ObjectType = union([literal('moveObject'), literal('package')]);
export type ObjectType = Infer<typeof ObjectType>;

export const SuiObjectRef = object({
  /** Base64 string representing the object digest */
  digest: TransactionDigest,
  /** Hex code as string representing the object id */
  objectId: string(),
  /** Object version */
  version: number(),
});
export type SuiObjectRef = Infer<typeof SuiObjectRef>;

export const SuiObjectInfo = assign(
  SuiObjectRef,
  object({
    type: string(),
    owner: ObjectOwner,
    previousTransaction: TransactionDigest,
  }),
);
export type SuiObjectInfo = Infer<typeof SuiObjectInfo>;

export const ObjectContentFields = record(string(), any());
export type ObjectContentFields = Infer<typeof ObjectContentFields>;

export const MovePackageContent = record(string(), string());
export type MovePackageContent = Infer<typeof MovePackageContent>;

export const SuiMoveObject = object({
  /** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
  type: string(),
  /** Fields and values stored inside the Move object */
  fields: ObjectContentFields,
  has_public_transfer: optional(boolean()),
});
export type SuiMoveObject = Infer<typeof SuiMoveObject>;

export const SuiMovePackage = object({
  /** A mapping from module name to disassembled Move bytecode */
  disassembled: MovePackageContent,
});
export type SuiMovePackage = Infer<typeof SuiMovePackage>;

export const SuiData = union([
  assign(SuiMoveObject, object({ dataType: literal('moveObject') })),
  assign(SuiMovePackage, object({ dataType: literal('package') })),
]);
export type SuiData = Infer<typeof SuiData>;

export const MIST_PER_SUI = BigInt(1000000000);

export const SuiObject = object({
  /** The meat of the object */
  data: SuiData,
  /** The owner of the object */
  owner: ObjectOwner,
  /** The digest of the transaction that created or last mutated this object */
  previousTransaction: TransactionDigest,
  /**
   * The amount of SUI we would rebate if this object gets deleted.
   * This number is re-calculated each time the object is mutated based on
   * the present storage gas price.
   */
  storageRebate: number(),
  reference: SuiObjectRef,
});
export type SuiObject = Infer<typeof SuiObject>;

export const ObjectStatus = union([
  literal('Exists'),
  literal('NotExists'),
  literal('Deleted'),
]);
export type ObjectStatus = Infer<typeof ObjectStatus>;

export const GetOwnedObjectsResponse = array(SuiObjectInfo);
export type GetOwnedObjectsResponse = Infer<typeof GetOwnedObjectsResponse>;

export const GetObjectDataResponse = object({
  status: ObjectStatus,
  details: union([SuiObject, ObjectId, SuiObjectRef]),
});
export type GetObjectDataResponse = Infer<typeof GetObjectDataResponse>;

export type ObjectDigest = string;
export type Order = 'ascending' | 'descending';

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* -------------------------- GetObjectDataResponse ------------------------- */

export function getObjectExistsResponse(
  resp: GetObjectDataResponse,
): SuiObject | undefined {
  return resp.status !== 'Exists' ? undefined : (resp.details as SuiObject);
}

export function getObjectDeletedResponse(
  resp: GetObjectDataResponse,
): SuiObjectRef | undefined {
  return resp.status !== 'Deleted' ? undefined : (resp.details as SuiObjectRef);
}

export function getObjectNotExistsResponse(
  resp: GetObjectDataResponse,
): ObjectId | undefined {
  return resp.status !== 'NotExists' ? undefined : (resp.details as ObjectId);
}

export function getObjectReference(
  resp: GetObjectDataResponse,
): SuiObjectRef | undefined {
  return (
    getObjectExistsResponse(resp)?.reference || getObjectDeletedResponse(resp)
  );
}

/* ------------------------------ SuiObjectRef ------------------------------ */

export function getObjectId(
  data: GetObjectDataResponse | SuiObjectRef,
): ObjectId {
  if ('objectId' in data) {
    return data.objectId;
  }
  return (
    getObjectReference(data)?.objectId ?? getObjectNotExistsResponse(data)!
  );
}

export function getObjectVersion(
  data: GetObjectDataResponse | SuiObjectRef,
): number | undefined {
  if ('version' in data) {
    return data.version;
  }
  return getObjectReference(data)?.version;
}

/* -------------------------------- SuiObject ------------------------------- */

export function getObjectType(
  resp: GetObjectDataResponse,
): ObjectType | undefined {
  return getObjectExistsResponse(resp)?.data.dataType;
}

export function getObjectPreviousTransactionDigest(
  resp: GetObjectDataResponse,
): TransactionDigest | undefined {
  return getObjectExistsResponse(resp)?.previousTransaction;
}

export function getObjectOwner(
  resp: GetObjectDataResponse,
): ObjectOwner | undefined {
  return getObjectExistsResponse(resp)?.owner;
}

export function getSharedObjectInitialVersion(
  resp: GetObjectDataResponse,
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
  resp: GetObjectDataResponse,
): string | undefined {
  return getMoveObject(resp)?.type;
}

export function getObjectFields(
  resp: GetObjectDataResponse | SuiMoveObject,
): ObjectContentFields | undefined {
  if ('fields' in resp) {
    return resp.fields;
  }
  return getMoveObject(resp)?.fields;
}

export function getMoveObject(
  data: GetObjectDataResponse | SuiObject,
): SuiMoveObject | undefined {
  const suiObject = 'data' in data ? data : getObjectExistsResponse(data);
  if (suiObject?.data.dataType !== 'moveObject') {
    return undefined;
  }
  return suiObject.data as SuiMoveObject;
}

export function hasPublicTransfer(
  data: GetObjectDataResponse | SuiObject,
): boolean {
  return getMoveObject(data)?.has_public_transfer ?? false;
}

export function getMovePackageContent(
  data: GetObjectDataResponse | SuiMovePackage,
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
