// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletAdapterProvider } from "@mysten/wallet-adapter-base";
import {
  isStandardWalletAdapterCompatibleWallet,
  StandardWalletAdapterWallet,
  DEPRECATED_getWallets,
  Wallets,
} from "@mysten/wallet-standard";
import { StandardWalletAdapter } from "./StandardWalletAdapter";
import mitt, { Emitter } from "mitt";

type Events = {
  changed: void;
};

export class WalletStandardAdapterProvider implements WalletAdapterProvider {
  #wallets: Wallets;
  #adapters: Map<StandardWalletAdapterWallet, StandardWalletAdapter>;
  #events: Emitter<Events>;

  constructor() {
    this.#adapters = new Map();
    this.#wallets = DEPRECATED_getWallets();
    this.#events = mitt();

    this.#wallets.on("register", () => {
      this.#events.emit("changed");
    });

    this.#wallets.on("unregister", () => {
      this.#events.emit("changed");
    });
  }

  get() {
    const filtered = this.#wallets
      .get()
      .filter(isStandardWalletAdapterCompatibleWallet);

    filtered.forEach((wallet) => {
      if (!this.#adapters.has(wallet)) {
        this.#adapters.set(wallet, new StandardWalletAdapter({ wallet }));
      }
    });

    return [...this.#adapters.values()];
  }

  on<T extends keyof Events>(
    eventName: T,
    callback: (data: Events[T]) => void
  ) {
    this.#events.on(eventName, callback);
    return () => {
      this.#events.off(eventName, callback);
    };
  }
}
