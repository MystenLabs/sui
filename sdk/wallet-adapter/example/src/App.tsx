// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { TransactionBlock } from "@mysten/sui.js";
import { useEffect } from "react";

<<<<<<< HEAD
function App() {
  const {
    currentWallet,
    currentAccount,
=======
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
>>>>>>> fork/testnet
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
<<<<<<< HEAD
            const txb = new TransactionBlock();
            const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
            txb.transferObjects([coin], txb.pure(currentAccount!.address));

            console.log(await signTransactionBlock({ transactionBlock: txb }));
=======
            console.log(await signTransactionBlock({ transactionBlock }));
>>>>>>> fork/testnet
          }}
        >
          Sign Transaction
        </button>
      </div>
      <div>
        <button
          onClick={async () => {
            const txb = new TransactionBlock();
            const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
            txb.transferObjects([coin], txb.pure(currentAccount!.address));

            console.log(
              await signAndExecuteTransactionBlock({
<<<<<<< HEAD
                transactionBlock: txb,
=======
                transactionBlock,
>>>>>>> fork/testnet
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
