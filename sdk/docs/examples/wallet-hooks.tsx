// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ConnectButton,
	SuiClientProvider,
	useAccounts,
	useAutoConnectWallet,
	useConnectWallet,
	useCurrentAccount,
	useCurrentWallet,
	useDisconnectWallet,
	useSignAndExecuteTransaction,
	useSignPersonalMessage,
	useSignTransaction,
	useSwitchAccount,
	useWallets,
	WalletProvider,
} from '@mysten/dapp-kit';
import { getFullnodeUrl } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ComponentProps } from 'react';
import { useEffect, useState } from 'react';

import '@mysten/dapp-kit/dist/index.css';

export const UseWalletsExample = withProviders(() => {
	const wallets = useWallets();

	return (
		<div>
			<h2>Installed wallets:</h2>
			{wallets.length === 0 && <div>No wallets installed</div>}
			<ul>
				{wallets.map((wallet) => (
					<li key={wallet.name}>- {wallet.name}</li>
				))}
			</ul>
		</div>
	);
});

export const UseAccountsExample = withProviders(() => {
	const accounts = useAccounts();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			<h2>Available accounts:</h2>
			{accounts.length === 0 && <div>No accounts detected</div>}
			<ul>
				{accounts.map((account) => (
					<li key={account.address}>- {account.address}</li>
				))}
			</ul>
		</div>
	);
});

export const UseCurrentWalletExample = withProviders(() => {
	const { currentWallet, connectionStatus } = useCurrentWallet();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			{connectionStatus === 'connected' ? (
				<div>
					<h2>Current wallet:</h2>
					<div>Name: {currentWallet.name}</div>
					<div>
						Accounts:
						<ul>
							{currentWallet.accounts.map((account) => (
								<li key={account.address}>- {account.address}</li>
							))}
						</ul>
					</div>
				</div>
			) : (
				<div>Connection status: {connectionStatus}</div>
			)}
		</div>
	);
});

export const UseCurrentAccountExample = withProviders(() => {
	const account = useCurrentAccount();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			{!account && <div>No account connected</div>}
			{account && (
				<div>
					<h2>Current account:</h2>
					<div>Address: {account.address}</div>
				</div>
			)}
		</div>
	);
});

export const UseAutoConnectionStatusExample = withProviders(
	() => {
		const autoConnectionStatus = useAutoConnectWallet();

		return (
			<div style={{ padding: 20 }}>
				<ConnectButton />
				<div>Auto-connection status: {autoConnectionStatus}</div>
			</div>
		);
	},
	{ autoConnect: true },
);

export const UseConnectWalletExample = withProviders(() => {
	const wallets = useWallets();
	const { mutate: connect } = useConnectWallet();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			<ul>
				{wallets.map((wallet) => (
					<li key={wallet.name}>
						<button
							onClick={() => {
								connect(
									{ wallet },
									{
										onSuccess: () => console.log('connected'),
									},
								);
							}}
						>
							Connect to {wallet.name}
						</button>
					</li>
				))}
			</ul>
		</div>
	);
});

export const UseDisconnectWalletExample = withProviders(() => {
	const { mutate: disconnect } = useDisconnectWallet();
	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			<div>
				<button onClick={() => disconnect()}>Disconnect</button>
			</div>
		</div>
	);
});

export const UseSwitchAccountExample = withProviders(() => {
	const { mutate: switchAccount } = useSwitchAccount();
	const accounts = useAccounts();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			<ul>
				{accounts.map((account) => (
					<li key={account.address}>
						<button
							onClick={() => {
								switchAccount(
									{ account },
									{
										onSuccess: () => console.log(`switched to ${account.address}`),
									},
								);
							}}
						>
							Switch to {account.address}
						</button>
					</li>
				))}
			</ul>
		</div>
	);
});

export const UseSignPersonalMessageExample = withProviders(() => {
	const { mutate: signPersonalMessage } = useSignPersonalMessage();
	const [message, setMessage] = useState('hello, World!');
	const [signature, setSignature] = useState('');
	const currentAccount = useCurrentAccount();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			{currentAccount && (
				<>
					<div>
						<label>
							Message:{' '}
							<input type="text" value={message} onChange={(ev) => setMessage(ev.target.value)} />
						</label>
					</div>
					<button
						onClick={() => {
							signPersonalMessage(
								{
									message: new TextEncoder().encode(message),
								},
								{
									onSuccess: (result) => {
										console.log('signed message', result);
										setSignature(result.signature);
									},
								},
							);
						}}
					>
						Sign message
					</button>
					<div>Signature: {signature}</div>
				</>
			)}
		</div>
	);
});

export const UseSignTransactionExample = withProviders(() => {
	const { mutate: signTransaction } = useSignTransaction();
	const [signature, setSignature] = useState('');
	const currentAccount = useCurrentAccount();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			{currentAccount && (
				<>
					<div>
						<button
							onClick={() => {
								signTransaction(
									{
										transaction: new Transaction(),
										chain: 'sui:devnet',
									},
									{
										onSuccess: (result) => {
											console.log('signed message', result);
											setSignature(result.signature);
										},
									},
								);
							}}
						>
							Sign empty transaction
						</button>
					</div>
					<div>Signature: {signature}</div>
				</>
			)}
		</div>
	);
});

export const UseSignAndExecuteTransactionExample = withProviders(() => {
	const { mutate: signAndExecuteTransaction } = useSignAndExecuteTransaction();
	const [digest, setDigest] = useState('');
	const currentAccount = useCurrentAccount();

	return (
		<div style={{ padding: 20 }}>
			<ConnectButton />
			{currentAccount && (
				<>
					<div>
						<button
							onClick={() => {
								signAndExecuteTransaction(
									{
										transaction: new Transaction(),
										chain: 'sui:devnet',
									},
									{
										onSuccess: (result) => {
											console.log('executed transaction', result);
											setDigest(result.digest);
										},
									},
								);
							}}
						>
							Sign and execute transaction
						</button>
					</div>
					<div>Digest: {digest}</div>
				</>
			)}
		</div>
	);
});

function withProviders(
	Component: React.FunctionComponent<object>,
	walletProviderProps?: Omit<ComponentProps<typeof WalletProvider>, 'children'>,
) {
	// Work around server-side pre-rendering
	const queryClient = new QueryClient();
	const networks = {
		mainnet: { url: getFullnodeUrl('mainnet') },
	};

	return () => {
		const [shouldRender, setShouldRender] = useState(false);
		useEffect(() => {
			setShouldRender(true);
		}, [setShouldRender]);

		if (!shouldRender) {
			return null;
		}

		return (
			<QueryClientProvider client={queryClient}>
				<SuiClientProvider networks={networks}>
					<WalletProvider {...walletProviderProps}>
						<Component />
					</WalletProvider>
				</SuiClientProvider>
			</QueryClientProvider>
		);
	};
}
