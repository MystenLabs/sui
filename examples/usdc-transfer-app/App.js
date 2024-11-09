// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#setup
import React, { useState, useEffect } from "react";
import {
  createNetworkConfig,
  SuiClientProvider,
  useSuiClient,
  ConnectButton,
  useCurrentAccount,
  useSignAndExecuteTransaction,
  WalletProvider,
} from "@mysten/dapp-kit";
import { Transaction } from "@mysten/sui/transactions";
import { getFullnodeUrl } from "@mysten/sui/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import "@mysten/dapp-kit/dist/index.css";
// docs::/#setup

const { networkConfig } = createNetworkConfig({
  testnet: {
    url: getFullnodeUrl("testnet"),
  },
  mainnet: {
    url: getFullnodeUrl("mainnet"),
  },
});

// Create a new QueryClient for managing and caching asynchronous queries
const queryClient = new QueryClient();

// Define the USDC token type on Sui Testnet
// This is the unique identifier for the USDC token on Sui
const USDC_TYPE = '0xa1ec7fc00a6f40db9693ad1415d0c193ad3906494428cf252621037bd7117e29::usdc::USDC';

function HomeContent() {
  // docs::#state
  // Use the wallet kit to get the current account and transaction signing function
  const currentAccount = useCurrentAccount();
  const { mutate: signAndExecuteTransaction } = useSignAndExecuteTransaction();
  // Get the Sui client for interacting with the Sui network
  const suiClient = useSuiClient();
  const [open, setOpen] = useState(false);
  const [connected, setConnected] = useState(false);
  const [amount, setAmount] = useState("");
  const [recipientAddress, setRecipientAddress] = useState("");
  const [txStatus, setTxStatus] = useState("");
  // docs::/#state

  // docs::#useeffect
  useEffect(() => {
    setConnected(!!currentAccount);
  }, [currentAccount]);
  // docs::/#useeffect

  const handleSendTokens = async () => {
    if (!currentAccount || !amount || !recipientAddress) {
      setTxStatus("Please connect wallet and fill in all fields");
      return;
    }
    try {
      // Fetch USDC coins owned by the current account
      // This uses the SuiClient to get coins of the specified type owned by the current address
      const { data: coins } = await suiClient.getCoins({
        owner: currentAccount.address,
        coinType: USDC_TYPE,
      });
      if (coins.length === 0) {
        setTxStatus("No USDC coins found in your wallet");
        return;
      }
      // Create a new transaction block
      // Transaction is used to construct and execute transactions on Sui
      const tx = new Transaction();
      // Convert amount to smallest unit (6 decimals)
      const amountInSmallestUnit = BigInt(parseFloat(amount) * 1_000_000);
      // Split the coin and get a new coin with the specified amount
      // This creates a new coin object with the desired amount to be transferred
      const [coin] = tx.splitCoins(coins[0].coinObjectId, [
        tx.pure.u64(amountInSmallestUnit),
      ]);
      // Transfer the split coin to the recipient
      // This adds a transfer operation to the transaction block
      tx.transferObjects([coin], tx.pure.address(recipientAddress));
      // Sign and execute the transaction block
      // This sends the transaction to the network and waits for it to be executed
      const result = await signAndExecuteTransaction(
        {
          transaction: tx,
        },
        {
          onSuccess: (result) => {
            console.log("Transaction result:", result);
            setTxStatus(`Transaction successful. Digest: ${result.digest}`);
          },
        }
      );
    } catch (error) {
      console.error("Error sending tokens:", error);
      setTxStatus(
        `Error: ${error instanceof Error ? error.message : "Unknown error"}`
      );
    }
  };

  // docs::#ui
  return (
    <main className="mainwrapper">
      <div className="outerwrapper">
        <h1 className="h1">Sui USDC Sender (Testnet)</h1>
        <ConnectButton />
        {connected && currentAccount && (
          <p className="status">Connected: {currentAccount.address}</p>
        )}
        <div className="form">
          <input
            type="text"
            placeholder="Amount (in USDC)"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            className="input"
          />
          <input
            type="text"
            placeholder="Recipient Address"
            value={recipientAddress}
            onChange={(e) => setRecipientAddress(e.target.value)}
            className="input"
          />
          <button
            onClick={handleSendTokens}
            disabled={!connected}
            className={`${
              connected && amount && recipientAddress
                ? "connected"
                : "notconnected"
            } transition`}
          >
            Send USDC
          </button>
        </div>
        {txStatus && <p className="status">{txStatus}</p>}
      </div>
    </main>
  );
  // docs::/#ui
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <SuiClientProvider networks={networkConfig} defaultNetwork="testnet">
        <WalletProvider>
          <HomeContent />
        </WalletProvider>
      </SuiClientProvider>
    </QueryClientProvider>
  );
}

export default App;
