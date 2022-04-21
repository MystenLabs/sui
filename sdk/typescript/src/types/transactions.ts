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
import { RawObjectRef, TransactionDigest } from './objects';

export type GatewayTxSeqNumber = Infer<typeof GatewayTxSeqNumber>;
export type GetTxnDigestsResponse = Infer<typeof GetTxnDigestsResponse>;
export type Transfer = Infer<typeof Transfer>;
export type MoveModulePublish = Infer<typeof MoveModulePublish>;
export type MoveCall = Infer<typeof MoveCall>;
export type MoveTypeTag = Infer<typeof MoveTypeTag>;
export type SingleTransactionKind = Infer<typeof SingleTransactionKind>;
export type TransactionKind = Infer<typeof TransactionKind>;
export type Transaction = Infer<typeof Transaction>;
export type TransactionData = Infer<typeof TransactionData>;
export type EmptySignInfo = Infer<typeof EmptySignInfo>;
export type RawAuthoritySignInfo = Infer<typeof RawAuthoritySignInfo>;
export type CertifiedTransaction = Infer<typeof CertifiedTransaction>;

export const GatewayTxSeqNumber = number();

export const GetTxnDigestsResponse = array(
  tuple([GatewayTxSeqNumber, TransactionDigest])
);

export const Transfer = pick({
  recipient: string(),
  object_ref: RawObjectRef,
});

export const MoveModulePublish = pick({
  modules: unknown(),
});

export const MoveTypeTag = enums([
  'bool',
  'u8',
  'u64',
  'u128',
  'address',
  'signer',
  'vector',
  'struct',
]);

export const MoveCall = pick({
  packages: RawObjectRef,
  module: string(),
  function: string(),
  type_arguments: array(MoveTypeTag),
  object_arguments: array(RawObjectRef),
  shared_object_arguments: array(string()),
  pure_arguments: array(unknown()),
});

export const SingleTransactionKind = union([
  pick({ Transfer: Transfer }),
  pick({ Publish: MoveModulePublish }),
  pick({ Call: MoveCall }),
]);

export const TransactionKind = union([
  pick({ Single: SingleTransactionKind }),
  pick({ Batch: array(SingleTransactionKind) }),
]);

export const TransactionData = pick({
  kind: TransactionKind,
  sender: string(),
  gas_payment: RawObjectRef,
  gas_budget: number(),
});

export const EmptySignInfo = object({});
export const RawAuthoritySignInfo = tuple([string(), string()]);

export const Transaction = pick({
  data: TransactionData,
  tx_signature: string(),
  auth_signature: EmptySignInfo,
});

export const CertifiedTransaction = pick({
  transaction: Transaction,
  signatures: array(RawAuthoritySignInfo),
});
