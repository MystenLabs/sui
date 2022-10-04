// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  MoveCallTransaction,
  SignableTransaction,
  SuiAddress,
  SuiTransactionResponse,
} from "@mysten/sui.js";

export interface WalletCapabilities {
  // Metadata
  name: string;
  connected: boolean;
  connecting: boolean;
  // Connection Management
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;

  /**
   * Suggest a transaction for the user to sign. Supports all valid transaction types.
   */
  signAndExecuteTransaction?(
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse>;

  getAccounts: () => Promise<SuiAddress[]>;

  /** @deprecated Prefer `signAndExecuteTransaction` when available. */
  executeMoveCall: (
    transaction: MoveCallTransaction
  ) => Promise<SuiTransactionResponse>;

  /** @deprecated Prefer `signAndExecuteTransaction` when available. */
  executeSerializedMoveCall: (
    transactionBytes: Uint8Array
  ) => Promise<SuiTransactionResponse>;
}

export type WalletAdapter = WalletCapabilities;
