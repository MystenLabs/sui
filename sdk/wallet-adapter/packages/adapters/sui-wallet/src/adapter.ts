// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  MoveCallTransaction,
  SignableTransaction,
  SuiAddress,
  SuiSignatureResponse,
  SuiTransactionResponse,
} from "@mysten/sui.js";
import { WalletAdapter } from "@mysten/wallet-adapter-base";

const ALL_PERMISSION_TYPES = ["viewAccount", "suggestTransactions", "suggestSignatures"] as const;
type AllPermissionsType = typeof ALL_PERMISSION_TYPES;
type PermissionType = AllPermissionsType[number];

interface SuiWallet {
  hasPermissions(permissions: readonly PermissionType[]): Promise<boolean>;
  requestPermissions(): Promise<boolean>;
  getAccounts(): Promise<SuiAddress[]>;
  signMessage(message: Uint8Array): Promise<SuiSignatureResponse>;
  executeMoveCall: (
    transaction: MoveCallTransaction
  ) => Promise<SuiTransactionResponse>;
  executeSerializedMoveCall: (
    transactionBytes: Uint8Array
  ) => Promise<SuiTransactionResponse>;
  signAndExecuteTransaction: (
    transaction: SignableTransaction
  ) => Promise<SuiTransactionResponse>;
}

interface SuiWalletWindow {
  suiWallet: SuiWallet;
}

declare const window: SuiWalletWindow;

/**
 * @deprecated This wallet adapter has been replaced by the `WalletStandardAdapterProvider`.
 */
export class SuiWalletAdapter implements WalletAdapter {
  connecting: boolean;
  connected: boolean;

  getAccounts(): Promise<string[]> {
    return window.suiWallet.getAccounts();
  }
  executeMoveCall(
    transaction: MoveCallTransaction
  ): Promise<SuiTransactionResponse> {
    return window.suiWallet.executeMoveCall(transaction);
  }
  executeSerializedMoveCall(
    transactionBytes: Uint8Array
  ): Promise<SuiTransactionResponse> {
    return window.suiWallet.executeSerializedMoveCall(transactionBytes);
  }
  signMessage(message: Uint8Array): Promise<SuiSignatureResponse> {
    return window.suiWallet.signMessage(message);
  }
  signAndExecuteTransaction(
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse> {
    return window.suiWallet.signAndExecuteTransaction(transaction);
  }

  name = "Sui Wallet (legacy)";

  async connect(): Promise<void> {
    this.connecting = true;
    if (window.suiWallet) {
      const wallet = window.suiWallet;
      try {
        await wallet.requestPermissions();
        const newLocal: readonly PermissionType[] = ["viewAccount", "suggestTransactions", "suggestSignatures"];
        await wallet.hasPermissions(newLocal);
        this.connected = true;
      } catch (err) {
        console.error(err);
      } finally {
        this.connecting = false;
      }
    }
  }

  // Come back to this later
  async disconnect(): Promise<void> {
    if (this.connected == true) {
      this.connected = false;
    }
    console.log("disconnected");
  }

  constructor() {
    this.connected = false;
    this.connecting = false;
  }
}
