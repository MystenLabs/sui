// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SignableTransaction,
  SuiAddress,
  SuiTransactionResponse,
} from "@mysten/sui.js";

export interface WalletAdapterEvents {
  changed(changes: { connected?: boolean; accounts?: SuiAddress[] }): void;
}

export interface WalletAdapter {
  // Metadata
  name: string;
  icon?: string;

  connected: boolean;
  connecting: boolean;
  // Connection Management
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  on: <E extends keyof WalletAdapterEvents>(
    event: E,
    callback: WalletAdapterEvents[E]
  ) => () => void;
  /**
   * Suggest a transaction for the user to sign. Supports all valid transaction types.
   */
  signAndExecuteTransaction(
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse>;

  getAccounts: () => Promise<SuiAddress[]>;
}

type WalletAdapterProviderUnsubscribe = () => void;

/**
 * An interface that can dynamically provide wallet adapters. This is useful for
 * cases where the list of wallet adapters is dynamic.
 */
export interface WalletAdapterProvider {
  /** Get a list of wallet adapters from this provider. */
  get(): WalletAdapter[];
  /** Detect changes to the list of wallet adapters. */
  on(
    eventName: "changed",
    callback: () => void
  ): WalletAdapterProviderUnsubscribe;
}

export type WalletAdapterOrProvider = WalletAdapterProvider | WalletAdapter;
export type WalletAdapterList = WalletAdapterOrProvider[];

export function isWalletAdapter(
  wallet: WalletAdapterOrProvider
): wallet is WalletAdapter {
  return "connect" in wallet;
}

export function isWalletProvider(
  wallet: WalletAdapterOrProvider
): wallet is WalletAdapterProvider {
  return !isWalletAdapter(wallet);
}

/**
 * Takes an array of wallet adapters and providers, and resolves it to a
 * flat list of wallet adapters.
 */
export function resolveAdapters(adapterAndProviders: WalletAdapterList) {
  return adapterAndProviders.flatMap((adapter) => {
    if (isWalletProvider(adapter)) {
      return adapter.get();
    }

    return adapter;
  });
}
