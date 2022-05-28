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

export type SuiObjectInfo = SuiObjectRef & {
  type: string;
  owner: ObjectOwner;
  previousTransaction: TransactionDigest;
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

export type GetOwnedObjectsResponse = SuiObjectInfo[];

export type GetObjectDataResponse = {
  status: ObjectStatus;
  details: SuiObject | ObjectId | SuiObjectRef;
};

export type ObjectDigest = string;
export type ObjectId = string;
export type SequenceNumber = number;

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

export function getMoveObjectType(
  resp: GetObjectDataResponse
): string | undefined {
  return getMoveObject(resp)?.type;
}

export function getObjectFields(
  resp: GetObjectDataResponse
): ObjectContentFields | undefined {
  return getMoveObject(resp)?.fields;
}

export function getMoveObject(
  resp: GetObjectDataResponse
): SuiMoveObject | undefined {
  const suiObject = getObjectExistsResponse(resp);
  if (suiObject?.data.dataType !== 'moveObject') {
    return undefined;
  }
  return suiObject.data as SuiMoveObject;
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
