// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { useEffect } from "react";

function App() {
  const { currentWallet, signTransaction } = useWalletKit();

  useEffect(() => {
    // You can do something with `currentWallet` here.
  }, [currentWallet]);

  return (
    <div className="App">
      <ConnectButton />
      <button
        onClick={async () => {
          console.log(
            await signTransaction({
              transaction: {
                kind: "moveCall",
                data: {
                  packageObjectId: "0x2",
                  module: "devnet_nft",
                  function: "mint",
                  typeArguments: [],
                  arguments: ["foo", "bar", "baz"],
                  gasBudget: 2000,
                },
              },
            })
          );
        }}
      >
        Sign
      </button>
    </div>
  );
}

export default App;
