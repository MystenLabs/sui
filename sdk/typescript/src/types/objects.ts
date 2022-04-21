// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  type as pick,
  string,
  Infer,
  number,
  enums,
  union,
  tuple,
  object,
  any,
  literal,
} from 'superstruct';
import {
  SuiAddress,
  SuiAddressSchema,
  TransactionDigest,
  TransactionDigestSchema,
} from './common';

export type ObjectDigest = Infer<typeof ObjectDigestSchema>;
export type ObjectId = Infer<typeof ObjectIdSchema>;
export type SequenceNumber = Infer<typeof SequenceNumberSchema>;
export type ObjectRef = Infer<typeof ObjectRefSchema>;
export type ObjectContent = Infer<typeof ObjectContentSchema>;
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
export type RawObjectRef = [ObjectId, SequenceNumber, ObjectDigest];
export type ObjectExistsInfo = {
  objectRef: ObjectRef;
  object: SuiObject;
};
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

export const ObjectContentSchema = any();
export const ObjectOwnerSchema = union([
  pick({ AddressOwner: SuiAddressSchema }),
  pick({ ObjectOwner: SuiAddressSchema }),
  literal('Shared'),
  literal('Immutable'),
]);
export const SuiObjectSchema = pick({
  contents: ObjectContentSchema,
  owner: ObjectOwnerSchema,
  tx_digest: TransactionDigestSchema,
});
export const ObjectExistsInfoSchema = pick({
  objectRef: ObjectRefSchema,
  object: SuiObjectSchema,
});

export const ObjectNotExistsInfoSchema = object({
  objectId: ObjectIdSchema,
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
