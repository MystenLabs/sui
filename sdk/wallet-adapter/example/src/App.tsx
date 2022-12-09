// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { useMemo } from "react";
import { WalletKitProvider, ConnectButton } from "@mysten/wallet-kit";
import {
  WalletStandardAdapterProvider,
  UnsafeBurnerWalletAdapter,
} from "@mysten/wallet-adapter-all-wallets";

function App() {
  const adapters = useMemo(
    () => [
      new WalletStandardAdapterProvider(),
      new UnsafeBurnerWalletAdapter(),
    ],
    []
  );

  return (
    <div className="App">
      <WalletKitProvider adapters={adapters}>
        <ConnectButton />
      </WalletKitProvider>
    </div>
  );
}

export default App;
