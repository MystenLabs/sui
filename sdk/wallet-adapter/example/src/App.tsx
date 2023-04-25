// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "./App.css";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { TransactionBlock } from "@mysten/sui.js";
import { useEffect } from "react";
import { QredoConnectButton } from "./QredoConnectButton";

function App() {
  const {
    currentWallet,
    currentAccount,
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
            const txb = new TransactionBlock();
            const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
            txb.transferObjects([coin], txb.pure(currentAccount!.address));

            console.log(await signTransactionBlock({ transactionBlock: txb }));
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
                transactionBlock: txb,
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
      <hr />
      <div>
        <h3>Qredo Connect</h3>
        <QredoConnectButton />
      </div>
    </div>
  );
}

export default App;
