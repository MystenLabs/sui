// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner } from './common';
import { TransactionDigest } from './common';

export type SuiObjectRef = {
  /** Base64 string representing the object digest */
  digest: TransactionDigest;
  /** Hex code as string representing the object id */
  objectId: string;
  /** Object version */
  version: number;
};

export type ObjectContentFields = Record<string, any>;

export type MovePackageContent = Record<string, string>;

export type SuiData = { dataType: ObjectType } & (
  | SuiMoveObject
  | SuiMovePackage
);

export type SuiMoveObject = {
  /** Move type (e.g., "0x2::Coin::Coin<0x2::SUI::SUI>") */
  type: string;
  /** Fields and values stored inside the Move object */
  fields: ObjectContentFields;
};

export type SuiMovePackage = {
  /** A mapping from module name to disassembled Move bytecode */
  disassembled: MovePackageContent;
};

export type SuiObject = {
  /** The meat of the object */
  data: SuiData;
  /** The owner of the object */
  owner: ObjectOwner;
  /** The digest of the transaction that created or last mutated this object */
  previousTransaction: TransactionDigest;
  /**
   * The amount of SUI we would rebate if this object gets deleted.
   * This number is re-calculated each time the object is mutated based on
   * the present storage gas price.
   */
  storageRebate: number;
  reference: SuiObjectRef;
};

export type ObjectStatus = 'Exists' | 'NotExists' | 'Deleted';
export type ObjectType = 'moveObject' | 'package';

export type GetOwnedObjectRefsResponse = {
  objects: SuiObjectRef[];
};

export type GetObjectInfoResponse = {
  status: ObjectStatus;
  details: SuiObject | ObjectId | SuiObjectRef;
};

export type ObjectDigest = string;
export type ObjectId = string;
export type SequenceNumber = number;

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* -------------------------- GetObjectInfoResponse ------------------------- */

export function getObjectExistsResponse(
  resp: GetObjectInfoResponse
): SuiObject | undefined {
  return resp.status !== 'Exists' ? undefined : (resp.details as SuiObject);
}

export function getObjectDeletedResponse(
  resp: GetObjectInfoResponse
): SuiObjectRef | undefined {
  return resp.status !== 'Deleted' ? undefined : (resp.details as SuiObjectRef);
}

export function getObjectNotExistsResponse(
  resp: GetObjectInfoResponse
): ObjectId | undefined {
  return resp.status !== 'NotExists' ? undefined : (resp.details as ObjectId);
}

export function getObjectReference(
  resp: GetObjectInfoResponse
): SuiObjectRef | undefined {
  return (
    getObjectExistsResponse(resp)?.reference || getObjectDeletedResponse(resp)
  );
}

/* ------------------------------ SuiObjectRef ------------------------------ */

export function getObjectId(
  data: GetObjectInfoResponse | SuiObjectRef
): ObjectId {
  if ('objectId' in data) {
    return data.objectId;
  }
  return (
    getObjectReference(data)?.objectId ?? getObjectNotExistsResponse(data)!
  );
}

export function getObjectVersion(
  data: GetObjectInfoResponse | SuiObjectRef
): number | undefined {
  if ('version' in data) {
    return data.version;
  }
  return getObjectReference(data)?.version;
}

/* -------------------------------- SuiObject ------------------------------- */

export function getObjectType(
  resp: GetObjectInfoResponse
): ObjectType | undefined {
  return getObjectExistsResponse(resp)?.data.dataType;
}

export function getObjectPreviousTransactionDigest(
  resp: GetObjectInfoResponse
): TransactionDigest | undefined {
  return getObjectExistsResponse(resp)?.previousTransaction;
}

export function getObjectOwner(
  resp: GetObjectInfoResponse
): ObjectOwner | undefined {
  return getObjectExistsResponse(resp)?.owner;
}

export function getMoveObjectType(
  resp: GetObjectInfoResponse
): string | undefined {
  return getMoveObject(resp)?.type;
}

export function getObjectFields(
  resp: GetObjectInfoResponse
): ObjectContentFields | undefined {
  return getMoveObject(resp)?.fields;
}

export function getMoveObject(
  resp: GetObjectInfoResponse
): SuiMoveObject | undefined {
  const suiObject = getObjectExistsResponse(resp);
  if (suiObject?.data.dataType !== 'moveObject') {
    return undefined;
  }
  return suiObject.data as SuiMoveObject;
}

export function getMovePackageContent(
  data: GetObjectInfoResponse | SuiMovePackage
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
