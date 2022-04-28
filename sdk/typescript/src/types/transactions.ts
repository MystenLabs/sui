// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionDigest } from './common';
import { RawObjectRef } from './objects';

export type Transfer = {
  recipient: string;
  object_ref: RawObjectRef;
};
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
};

export type CertifiedTransaction = {
  transaction: Transaction;
  signatures: RawAuthoritySignInfo[];
};

export type GatewayTxSeqNumber = number;

export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest][];

export type MoveModulePublish = {
  modules: any;
};

export type MoveTypeTag =
  | 'bool'
  | 'u8'
  | 'u64'
  | 'u128'
  | 'address'
  | 'signer'
  | 'vector'
  | 'struct';

export type MoveCall = {
  packages: RawObjectRef;
  module: string;
  function: string;
  type_arguments: MoveTypeTag[];
  object_arguments: RawObjectRef[];
  shared_object_arguments: string[];
  pure_arguments: any[];
};

export type EmptySignInfo = object;
export type AuthorityName = string;
export type AuthoritySignature = string;
