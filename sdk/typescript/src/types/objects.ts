// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectExistsInfo, ObjectNotExistsInfo, ObjectRef, ObjectStatus } from "../providers/provider";

export type GetObjectInfoResponse = {
  status: ObjectStatus;
  details: ObjectExistsInfo | ObjectNotExistsInfo | ObjectRef;
};

export type ObjectDigestSchema = string;
export type ObjectIdSchema = string;
export type SequenceNumberSchema = number;

export type ObjectRefSchema = {
  digest: ObjectDigestSchema,
  objectId: ObjectIdSchema,
  version: SequenceNumberSchema,
};

// TODO: get rid of this by implementing some conversion logic from ObjectRef
export type RawObjectRefSchema = [
  ObjectIdSchema,
  SequenceNumberSchema,
  ObjectDigestSchema,
];

export type ObjectExistsInfoSchema = {
  objectRef: ObjectRefSchema,
  object: any,
};

export type ObjectNotExistsInfoSchema = {
  objectId: string,
};

export type ObjectStatusSchema = 'Exists' | 'NotExists' | 'Deleted';

export type GetObjectInfoResponseSchema = {
  status: ObjectStatusSchema,
  details: ObjectExistsInfoSchema | ObjectNotExistsInfoSchema | ObjectRefSchema
};
