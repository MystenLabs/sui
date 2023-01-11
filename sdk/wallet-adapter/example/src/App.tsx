// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { WalletKitProvider, ConnectButton } from "@mysten/wallet-kit";

function App() {
  return (
    <div className="App">
      <WalletKitProvider>
        <ConnectButton />
      </WalletKitProvider>
    </div>
  );
}

export default App;
