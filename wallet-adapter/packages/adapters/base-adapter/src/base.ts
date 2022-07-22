// Contains 

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