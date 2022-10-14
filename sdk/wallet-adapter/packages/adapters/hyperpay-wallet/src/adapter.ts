// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  MoveCallTransaction,
  SignableTransaction,
  SuiAddress,
  SuiTransactionResponse,
} from "@mysten/sui.js";
import { WalletAdapter } from "@mysten/wallet-adapter-base";

const ALL_PERMISSION_TYPES = ["viewAccount", "suggestTransactions"] as const;
type AllPermissionsType = typeof ALL_PERMISSION_TYPES;
type PermissionType = AllPermissionsType[number];

interface HyperPayWallet {
  hasPermissions(permissions: readonly PermissionType[]): Promise<boolean>;
  requestPermissions(): Promise<boolean>;
  getAccounts(): Promise<SuiAddress[]>;
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

interface HyperPayWalletWindow {
  hyperpayWallet: HyperPayWallet;
}

declare const window: HyperPayWalletWindow;

/**
 * @deprecated This wallet adapter has been replaced by the `WalletStandardAdapterProvider`.
 */
export class HyperPayWalletAdapter implements WalletAdapter {
  connecting: boolean;
  connected: boolean;

  getAccounts(): Promise<string[]> {
    return window.hyperpayWallet.getAccounts();
  }
  executeMoveCall(
    transaction: MoveCallTransaction
  ): Promise<SuiTransactionResponse> {
    return window.hyperpayWallet.executeMoveCall(transaction);
  }
  executeSerializedMoveCall(
    transactionBytes: Uint8Array
  ): Promise<SuiTransactionResponse> {
    return window.hyperpayWallet.executeSerializedMoveCall(transactionBytes);
  }
  signAndExecuteTransaction(
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse> {
    return window.hyperpayWallet.signAndExecuteTransaction(transaction);
  }

  name = "HyperPay Wallet";

  async connect(): Promise<void> {
    this.connecting = true;
    if (window.hyperpayWallet) {
      const wallet = window.hyperpayWallet;
      try {
        let given = await wallet.requestPermissions();
        const newLocal: readonly PermissionType[] = ["viewAccount"];
        let perms = await wallet.hasPermissions(newLocal);
        console.log(perms);
        console.log(given);
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
