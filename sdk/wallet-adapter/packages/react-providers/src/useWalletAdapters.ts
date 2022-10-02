// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  WalletAdapterList,
  isWalletProvider,
  resolveAdapters,
} from "@mysten/wallet-adapter-base";
import { useEffect, useState } from "react";

export function useWalletAdapters(adapterAndProviders: WalletAdapterList) {
  const [wallets, setWallets] = useState(() =>
    resolveAdapters(adapterAndProviders)
  );

  useEffect(() => {
    const providers = adapterAndProviders.filter(isWalletProvider);
    if (!providers.length) return;

    // Re-resolve the adapters just in case a provider has injected
    // before we've been able to attach an event listener:
    setWallets(resolveAdapters(adapterAndProviders));

    const listeners = providers.map((provider) =>
      provider.on("changed", () => {
        setWallets(resolveAdapters(adapterAndProviders));
      })
    );

    return () => {
      listeners.forEach((unlisten) => unlisten());
    };
  }, [adapterAndProviders]);

  return wallets;
}
