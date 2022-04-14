// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';

///////////////////////////////
// Exported Types
export interface TransferTransaction {
  fromAddress: string;
  objectId: string;
  toAddress: string;
  gasObjectId: string;
  gas_budget: number;
}

///////////////////////////////
// Exported Abstracts
/**
 * Serializes a transaction to a string that can be signed by a `Signer`.
 */
export interface TxnDataSerializer {
  new_transfer(transaction: TransferTransaction): Promise<Base64DataBuffer>;

  // TODO: add more interface methods
}
