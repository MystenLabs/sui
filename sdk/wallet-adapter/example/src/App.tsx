// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { WalletKitProvider, ConnectButton, useWalletKit } from "@mysten/wallet-kit";

function App() {
  const { currentWallet } = useWalletKit();
  console.log(currentWallet);
  return (
    <div className="App">
      <ConnectButton />
    </div>
  );
}

export default App;
