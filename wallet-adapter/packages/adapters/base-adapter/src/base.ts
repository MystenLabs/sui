// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MoveCallTransaction, SuiAddress, TransactionResponse } from "@mysten/sui.js";

export interface WalletCapabilities {
    // Metadata
    name: string;
    connected: boolean;
    connecting: boolean;
    // Connection Management
    connect: () => Promise<void>;
    disconnect: () => Promise<void>;
    // DappInterfaces
    getAccounts: () => Promise<SuiAddress[]>; 
    executeMoveCall: (transaction: MoveCallTransaction) => Promise<TransactionResponse>;
    executeSerializedMoveCall: (transactionBytes: Uint8Array) => Promise<TransactionResponse>;
}

export type WalletAdapter = WalletCapabilities;