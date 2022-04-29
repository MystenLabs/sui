// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionDigest } from "./transactions";

export type ObjectRef = {
  digest: TransactionDigest,
  objectId: string,
  version: number,
};

export type ObjectExistsInfo = {
  objectRef: ObjectRef,
  objectType: ObjectType,
  object: any,
};

export type ObjectNotExistsInfo = {
  objectId: any,
};

export type ObjectStatus = 'Exists' | 'NotExists' | 'Deleted';
export type ObjectType = 'moveObject' | 'movePackage';


export type GetOwnedObjectRefsResponse = {
  objects: ObjectRef[]
};

export type GetObjectInfoResponse = {
  status: ObjectStatus;
  details: ObjectExistsInfo | ObjectNotExistsInfo | ObjectRef;
};

export type ObjectDigest = string;
export type ObjectId = string;
export type SequenceNumber = number;

// TODO: get rid of this by implementing some conversion logic from ObjectRef
export type RawObjectRef = [
  ObjectId,
  SequenceNumber,
  ObjectDigest,
];
