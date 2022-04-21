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
  object,
} from 'superstruct';

export type ObjectDigest = Infer<typeof ObjectDigestSchema>;
export type ObjectId = Infer<typeof ObjectIdSchema>;
export type SequenceNumber = Infer<typeof SequenceNumberSchema>;
export type ObjectRef = Infer<typeof ObjectRefSchema>;
export type RawObjectRef = [ObjectId, SequenceNumber, ObjectDigest];
export type ObjectExistsInfo = Infer<typeof ObjectExistsInfoSchema>;
export type ObjectNotExistsInfo = Infer<typeof ObjectNotExistsInfoSchema>;
export type ObjectStatus = Infer<typeof ObjectStatusSchema>;
export type GetObjectInfoResponse = {
  status: ObjectStatus;
  details: ObjectExistsInfo | ObjectNotExistsInfo | ObjectRef;
};

export const ObjectDigestSchema = string();
export const ObjectIdSchema = string();
export const SequenceNumberSchema = number();

export const ObjectRefSchema = pick({
  digest: ObjectDigestSchema,
  objectId: ObjectIdSchema,
  version: SequenceNumberSchema,
});

// TODO: get rid of this by implementing some conversion logic from ObjectRef
export const RawObjectRefSchema = tuple([
  ObjectIdSchema,
  SequenceNumberSchema,
  ObjectDigestSchema,
]);

export const ObjectExistsInfoSchema = pick({
  objectRef: ObjectRefSchema,
  object: unknown(),
});

export const ObjectNotExistsInfoSchema = object({
  objectId: string(),
});

export const ObjectStatusSchema = enums(['Exists', 'NotExists', 'Deleted']);

export const GetObjectInfoResponseSchema = pick({
  status: ObjectStatusSchema,
  details: union([
    ObjectExistsInfoSchema,
    ObjectNotExistsInfoSchema,
    ObjectRefSchema,
  ]),
});
