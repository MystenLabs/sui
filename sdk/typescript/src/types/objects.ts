// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner } from './common';
import { TransactionDigest } from './common';

export type ObjectRef = {
  digest: TransactionDigest;
  objectId: string;
  version: number;
};

export type ObjectContentField =
  | ObjectContent
  | string
  | boolean
  | number
  | number[]
  | ObjectContent[];

export type ObjectContentFields = Record<string, ObjectContentField>;

export type ObjectContent = {
  fields: ObjectContentFields;
  type: string;
};

export type MovePackageContent = Record<string, string>;

export type SuiObject = {
  contents: ObjectContent | MovePackageContent;
  owner: ObjectOwner;
  tx_digest: TransactionDigest;
};

export type ObjectExistsInfo = {
  objectRef: ObjectRef;
  objectType: ObjectType;
  object: SuiObject;
};

export type ObjectNotExistsInfo = {
  objectId: ObjectId;
};

export type ObjectStatus = 'Exists' | 'NotExists' | 'Deleted';
export type ObjectType = 'moveObject' | 'movePackage';

export type GetOwnedObjectRefsResponse = {
  objects: ObjectRef[];
};

export type GetObjectInfoResponse = {
  status: ObjectStatus;
  details: ObjectExistsInfo | ObjectNotExistsInfo | ObjectRef;
};

export type ObjectDigest = string;
export type ObjectId = string;
export type SequenceNumber = number;

// TODO: get rid of this by implementing some conversion logic from ObjectRef
export type RawObjectRef = [ObjectId, SequenceNumber, ObjectDigest];

/* ---------------------------- Helper functions ---------------------------- */

export function getObjectExistsResponse(
  resp: GetObjectInfoResponse
): ObjectExistsInfo | undefined {
  return resp.status !== 'Exists'
    ? undefined
    : (resp.details as ObjectExistsInfo);
}

export function getObjectType(
  resp: GetObjectInfoResponse
): 'moveObject' | 'movePackage' | undefined {
  return getObjectExistsResponse(resp)?.objectType;
}

export function getObjectContent(
  resp: GetObjectInfoResponse
): ObjectContent | undefined {
  const existsInfo = getObjectExistsResponse(resp);
  if (existsInfo == null) {
    return undefined;
  }
  const { object, objectType } = existsInfo;
  return objectType === 'moveObject'
    ? (object.contents as ObjectContent)
    : undefined;
}

export function getMovePackageContent(
  resp: GetObjectInfoResponse
): MovePackageContent | undefined {
  const existsInfo = getObjectExistsResponse(resp);
  if (existsInfo == null) {
    return undefined;
  }
  const { object, objectType } = existsInfo;
  return objectType === 'movePackage'
    ? (object.contents as MovePackageContent)
    : undefined;
}
