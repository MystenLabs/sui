// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { TransactionBlock } from "@mysten/sui.js";
import { useEffect } from "react";

const transactionBlock = new TransactionBlock();
transactionBlock.moveCall({
  target: `0x2::devnet_nft::mint`,
  arguments: [
    transactionBlock.pure("foo"),
    transactionBlock.pure("bar"),
    transactionBlock.pure("baz"),
  ],
});

function App() {
  const {
    currentWallet,
    signTransactionBlock,
    signAndExecuteTransactionBlock,
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
            console.log(await signTransactionBlock({ transactionBlock }));
          }}
        >
          Sign Transaction
        </button>
      </div>
      <div>
        <button
          onClick={async () => {
            console.log(
              await signAndExecuteTransactionBlock({
                transactionBlock,
                options: { showEffects: true },
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
