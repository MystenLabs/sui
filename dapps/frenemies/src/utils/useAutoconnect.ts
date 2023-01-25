// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useEffect, useRef } from "react";

const AUTOCONNECT_KEY = "frenemies:wallet";

export function useAutoconnect() {
  const { wallets, currentWallet, connect } = useWalletKit();
  const previousWallet = useRef(currentWallet);

  useEffect(() => {
    const storedWallet = localStorage.getItem(AUTOCONNECT_KEY);
    // Initial load:
    if (!currentWallet && !previousWallet.current && storedWallet) {
      connect(storedWallet);
    }
    // Disconnect:
    if (!currentWallet && previousWallet.current) {
      localStorage.removeItem(AUTOCONNECT_KEY);
    }
    // Connect:
    if (currentWallet) {
      localStorage.setItem(AUTOCONNECT_KEY, currentWallet.name);
    }

    previousWallet.current = currentWallet;
  }, [wallets, currentWallet]);
}
