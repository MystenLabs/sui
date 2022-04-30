// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress, TransactionDigest } from './common';
import { ObjectId, RawObjectRef } from './objects';

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

// TODO: support u64
export type EpochId = number;

export type CertifiedTransaction = {
  epoch: EpochId;
  transaction: Transaction;
  signatures: RawAuthoritySignInfo[];
};

export type GatewayTxSeqNumber = number;

export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest][];

export type MoveModulePublish = {
  modules: any;
};

export type StructTag = {
  address: SuiAddress;
  module: string;
  name: string;
  type_args: MoveTypeTag[];
};
export type MoveTypeTag =
  | 'bool'
  | 'u8'
  | 'u64'
  | 'u128'
  | 'address'
  | 'signer'
  | { vector: MoveTypeTag[] }
  | { struct: StructTag };

export type MoveCall = {
  package: RawObjectRef;
  module: string;
  function: string;
  type_arguments: MoveTypeTag[];
  arguments: MoveCallArg[];
};

export type MoveCallArg =
  // TODO: convert to Uint8Array
  | { Pure: number[] }
  | { ImmOrOwnedObject: RawObjectRef }
  | { SharedObject: ObjectId };

export type EmptySignInfo = object;
export type AuthorityName = string;
export type AuthoritySignature = string;
