// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::https://docs.sui.io/guides/developer/stablecoins
// docs::#setup
'use client';

import { SuiClientProvider, useSuiClient } from '@mysten/dapp-kit';
import { Transaction } from '@mysten/sui/transactions';
import { ConnectButton, useWalletKit, WalletKitProvider } from '@mysten/wallet-kit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

// Define the network we're connecting to (testnet in this case)
const networks = {
	testnet: { url: 'https://fullnode.testnet.sui.io:443' },
};

// Create a new QueryClient for managing and caching asynchronous queries
const queryClient = new QueryClient();
// docs::/#setup

// Define the USDC token type on Sui Testnet
// This is the unique identifier for the USDC token on Sui
const USDC_TYPE = '0xa1ec7fc00a6f40db9693ad1415d0c193ad3906494428cf252621037bd7117e29::usdc::USDC';

function HomeContent() {
	// docs::#state
	// Use the wallet kit to get the current account and transaction signing function
	const { currentAccount, signAndExecuteTransactionBlock } = useWalletKit();
	// Get the Sui client for interacting with the Sui network
	const suiClient = useSuiClient();
	const [connected, setConnected] = useState(false);
	const [amount, setAmount] = useState('');
	const [recipientAddress, setRecipientAddress] = useState('');
	const [txStatus, setTxStatus] = useState('');
	// docs::/#state

	// docs::#useeffect
	// Update the connection status when the current account changes
	useEffect(() => {
		setConnected(!!currentAccount);
	}, [currentAccount]);
	// docs::/#useeffect

	const handleSendTokens = async () => {
		if (!currentAccount || !amount || !recipientAddress) {
			setTxStatus('Please connect wallet and fill all fields');
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
				setTxStatus('No USDC coins found in your wallet');
				return;
			}
			// Create a new transaction block
			// TransactionBlock is used to construct and execute transactions on Sui
			const tx = new Transaction();
			// Convert amount to smallest unit (6 decimals)
			const amountInSmallestUnit = BigInt(parseFloat(amount) * 1_000_000);
			// Split the coin and get a new coin with the specified amount
			// This creates a new coin object with the desired amount to be transferred
			const [coin] = tx.splitCoins(coins[0].coinObjectId, [tx.pure(amountInSmallestUnit)]);
			// Transfer the split coin to the recipient
			// This adds a transfer operation to the transaction block
			tx.transferObjects([coin], tx.pure(recipientAddress));
			// Sign and execute the transaction block
			// This sends the transaction to the network and waits for it to be executed
			const result = await signAndExecuteTransactionBlock({
				transactionBlock: tx,
			});
			console.log('Transaction result:', result);
			setTxStatus(`Transaction successful. Digest: ${result.digest}`);
		} catch (error) {
			console.error('Error sending tokens:', error);
			setTxStatus(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
		}
	};

	// docs::#ui
	return (
		<main className="flex min-h-screen flex-col items-center justify-center p-24">
			<div className="z-10 w-full max-w-5xl items-center justify-between font-mono text-sm">
				<h1 className="text-4xl font-bold mb-8">Sui USDC Sender (Testnet)</h1>
				<ConnectButton />
				{connected && currentAccount && <p className="mt-4">Connected: {currentAccount.address}</p>}
				<div className="mt-8">
					<input
						type="text"
						placeholder="Amount (in USDC)"
						value={amount}
						onChange={(e) => setAmount(e.target.value)}
						className="p-2 border rounded mr-2 text-black"
					/>
					<input
						type="text"
						placeholder="Recipient Address"
						value={recipientAddress}
						onChange={(e) => setRecipientAddress(e.target.value)}
						className="p-2 border rounded mr-2 text-black"
					/>
					<button
						onClick={handleSendTokens}
						disabled={!connected}
						className={`p-2 rounded ${
							connected && amount && recipientAddress
								? 'bg-blue-200 text-black hover:bg-blue-300'
								: 'bg-gray-300 text-gray-500'
						} transition-colors duration-200`}
					>
						Send USDC
					</button>
				</div>
				{txStatus && <p className="mt-4">{txStatus}</p>}
			</div>
		</main>
	);
	// docs::/#ui
}

export default function Home() {
	return (
		// Wrap the app with necessary providers
		// QueryClientProvider: Provides React Query context for managing async queries
		// SuiClientProvider: Provides the Sui client context for interacting with the Sui network
		// WalletKitProvider: Provides wallet connection and interaction capabilities
		<QueryClientProvider client={queryClient}>
			<SuiClientProvider networks={networks} defaultNetwork="testnet">
				<WalletKitProvider>
					<HomeContent />
				</WalletKitProvider>
			</SuiClientProvider>
		</QueryClientProvider>
	);
}
