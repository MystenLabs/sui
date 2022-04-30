// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from './common';
import { TransactionDigest } from './common';

export type ObjectRef = {
  digest: TransactionDigest;
  objectId: string;
  version: number;
};

export type ObjectContent = {
  fields: Record<string, ObjectContent | string | boolean | number>;
  type: string;
};
export type ObjectOwner =
  | { AddressOwner: SuiAddress }
  | { ObjectOwner: SuiAddress }
  | 'Shared'
  | 'Immutable';

export type SuiObject = {
  contents: ObjectContent;
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
