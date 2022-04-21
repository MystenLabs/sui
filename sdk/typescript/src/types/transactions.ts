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
  array,
  tuple,
  object,
} from 'superstruct';
import { RawObjectRef, RawObjectRefSchema } from './objects';

export type TransactionDigest = Infer<typeof TransactionDigestSchema>;
export type GatewayTxSeqNumber = Infer<typeof GatewayTxSeqNumberSchema>;
export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest][];
export type Transfer = {
  recipient: string;
  object_ref: RawObjectRef;
};
export type MoveModulePublish = Infer<typeof MoveModulePublishSchema>;
export type MoveCall = Infer<typeof MoveCallSchema>;
export type MoveTypeTag = Infer<typeof MoveTypeTagSchema>;
export type EmptySignInfo = Infer<typeof EmptySignInfoSchema>;
export type AuthorityName = Infer<typeof AuthorityNameSchema>;
export type AuthoritySignature = Infer<typeof AuthoritySignatureSchema>;
export type RawAuthoritySignInfo = [AuthorityName, AuthoritySignature];
export type SingleTransactionKind =
  | { Transfer: Transfer }
  | { Publish: MoveModulePublish }
  | { Call: MoveCall };
export type TransactionKind =
  | { Single: SingleTransactionKind }
  | { Batch: SingleTransactionKind[] };
export type TransactionData = {
  kind: TransactionKind;
  sender: string;
  gas_payment: RawObjectRef;
  gas_budget: number;
};
export type Transaction = {
  data: TransactionData;
  tx_signature: string;
  auth_signature: string;
};

export type CertifiedTransaction = {
  transaction: Transaction;
  signatures: RawAuthoritySignInfo[];
};

export const TransactionDigestSchema = string();
export const GatewayTxSeqNumberSchema = number();

export const GetTxnDigestsResponseSchema = array(
  tuple([GatewayTxSeqNumberSchema, TransactionDigestSchema])
);

export const TransferSchema = pick({
  recipient: string(),
  object_ref: RawObjectRefSchema,
});

export const MoveModulePublishSchema = pick({
  modules: unknown(),
});

export const MoveTypeTagSchema = enums([
  'bool',
  'u8',
  'u64',
  'u128',
  'address',
  'signer',
  'vector',
  'struct',
]);

export const MoveCallSchema = pick({
  packages: RawObjectRefSchema,
  module: string(),
  function: string(),
  type_arguments: array(MoveTypeTagSchema),
  object_arguments: array(RawObjectRefSchema),
  shared_object_arguments: array(string()),
  pure_arguments: array(unknown()),
});

export const SingleTransactionKindSchema = union([
  pick({ Transfer: TransferSchema }),
  pick({ Publish: MoveModulePublishSchema }),
  pick({ Call: MoveCallSchema }),
]);

export const TransactionKindSchema = union([
  pick({ Single: SingleTransactionKindSchema }),
  pick({ Batch: array(SingleTransactionKindSchema) }),
]);

export const TransactionDataSchema = pick({
  kind: TransactionKindSchema,
  sender: string(),
  gas_payment: RawObjectRefSchema,
  gas_budget: number(),
});

export const EmptySignInfoSchema = object({});
export const AuthorityNameSchema = string();
export const AuthoritySignatureSchema = string();
export const RawAuthoritySignInfoSchema = tuple([
  AuthorityNameSchema,
  AuthoritySignatureSchema,
]);

export const TransactionSchema = pick({
  data: TransactionDataSchema,
  tx_signature: string(),
  auth_signature: EmptySignInfoSchema,
});

export const CertifiedTransactionSchema = pick({
  transaction: TransactionSchema,
  signatures: array(RawAuthoritySignInfoSchema),
});
