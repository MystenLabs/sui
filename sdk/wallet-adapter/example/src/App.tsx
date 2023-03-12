// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { Commands, Transaction } from "@mysten/sui.js";
import { useEffect } from "react";

const transaction = new Transaction();
transaction.setGasBudget(2000);
transaction.add(
  Commands.MoveCall({
    target: `0x2::devnet_nft::mint`,
    arguments: [
      transaction.input("foo"),
      transaction.input("bar"),
      transaction.input("baz"),
    ],
  })
);

function App() {
  const {
    currentWallet,
    signTransaction,
    signAndExecuteTransaction,
    signMessage,
  } = useWalletKit();

  useEffect(() => {
    // You can do something with `currentWallet` here.
  }, [currentWallet]);

  return (
    <div className="App">
      <ConnectButton />
      <div>
        <button
          onClick={async () => {
            console.log(await signTransaction({ transaction }));
          }}
        >
          Sign Transaction
        </button>
      </div>
      <div>
        <button
          onClick={async () => {
            console.log(
              await signAndExecuteTransaction({
                transaction,
                options: { contentOptions: { showEffect: true } },
              })
            );
          }}
        >
          Sign + Execute Transaction
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
