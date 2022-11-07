// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { useMemo } from "react";
import { WalletProvider } from "@mysten/wallet-adapter-react";
import { WalletStandardAdapterProvider, UnsafeBurnerWalletAdapter } from "@mysten/wallet-adapter-all-wallets";
import { WalletWrapper } from "@mysten/wallet-adapter-react-ui";
import { TestButton } from "./TestButton";

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
      <header className="App-header">
        <WalletProvider adapters={adapters}>
          <TestButton />
          <WalletWrapper />
        </WalletProvider>
      </header>
    </div>
  );
}

export default App;
