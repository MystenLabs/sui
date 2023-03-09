// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { useEffect } from "react";

function App() {
  const { currentWallet, signTransaction, signMessage } = useWalletKit();

  useEffect(() => {
    // You can do something with `currentWallet` here.
  }, [currentWallet]);

  return (
    <div className="App">
      <ConnectButton />
      <div>
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
      <div>
        <button
          onClick={async () => {
            console.log(
              await signMessage({
                message: new TextEncoder().encode("Message to sign"),
              })
            );
          }}
        >
          Sign message
        </button>
      </div>
    </div>
  );
}

export default App;
