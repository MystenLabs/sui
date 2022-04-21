// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  type as pick,
  string,
  Infer,
  number,
  enums,
  unknown,
  union,
  tuple,
} from 'superstruct';

export type ObjectRef = Infer<typeof ObjectRef>;
export type RawObjectRef = Infer<typeof RawObjectRef>;
export type ObjectExistsInfo = Infer<typeof ObjectExistsInfo>;
export type ObjectNotExistsInfo = Infer<typeof ObjectNotExistsInfo>;
export type ObjectStatus = Infer<typeof ObjectStatus>;
export type GetObjectInfoResponse = Infer<typeof GetObjectInfoResponse>;
export type TransactionDigest = Infer<typeof TransactionDigest>;

export const TransactionDigest = string();

export const ObjectRef = pick({
  digest: TransactionDigest,
  objectId: string(),
  version: number(),
});

// TODO: get rid of this by implementing some conversion logic from ObjectRef
export const RawObjectRef = tuple([string(), number(), string()]);

export const ObjectExistsInfo = pick({
  objectRef: ObjectRef,
  object: unknown(),
});

export const ObjectNotExistsInfo = pick({
  objectId: string(),
});

export const ObjectStatus = enums(['Exists', 'NotExists', 'Deleted']);

export const GetObjectInfoResponse = pick({
  status: ObjectStatus,
  details: union([ObjectExistsInfo, ObjectNotExistsInfo, ObjectRef]),
});
