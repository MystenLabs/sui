// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignableTransaction } from "@mysten/sui.js";
import { WalletAdapter } from "@mysten/wallet-adapter-base";
import { StandardWalletAdapterWallet } from "@mysten/wallet-standard";

export interface StandardWalletAdapterConfig {
  wallet: StandardWalletAdapterWallet;
}

export class StandardWalletAdapter implements WalletAdapter {
  connected = false;
  connecting = false;

  #wallet: StandardWalletAdapterWallet;

  constructor({ wallet }: StandardWalletAdapterConfig) {
    this.#wallet = wallet;
  }

  get name() {
    return this.#wallet.name;
  }

  get icon() {
    return this.#wallet.icon;
  }

  get wallet() {
    return this.#wallet;
  }

  async getAccounts() {
    return this.#wallet.accounts.map((account) => account.address);
  }

  async connect() {
    try {
      if (this.connected || this.connecting) return;
      this.connecting = true;

      if (!this.#wallet.accounts.length) {
        await this.#wallet.features["standard:connect"].connect();
      }

      if (!this.#wallet.accounts.length) {
        throw new Error("No wallet accounts found");
      }

      this.connected = true;
    } finally {
      this.connecting = false;
    }
  }

  async disconnect() {
    this.connected = false;
    this.connecting = false;
    if (this.#wallet.features["standard:disconnect"]) {
      await this.#wallet.features["standard:disconnect"].disconnect();
    }
  }

  async signAndExecuteTransaction(transaction: SignableTransaction) {
    return this.#wallet.features[
      "sui:signAndExecuteTransaction"
    ].signAndExecuteTransaction({
      transaction,
    });
  }
}
