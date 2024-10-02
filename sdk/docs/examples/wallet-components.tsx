// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ConnectButton,
	ConnectModal,
	SuiClientProvider,
	useCurrentAccount,
	WalletProvider,
} from '@mysten/dapp-kit';
import { getFullnodeUrl } from '@mysten/sui/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import '@mysten/dapp-kit/dist/index.css';

export const ConnectButtonExample = withProviders(() => {
	return <ConnectButton />;
});

export const ControlledConnectModalExample = withProviders(() => {
	const currentAccount = useCurrentAccount();
	const [open, setOpen] = useState(false);

	return (
		<ConnectModal
			trigger={
				<button disabled={!!currentAccount}> {currentAccount ? 'Connected' : 'Connect'}</button>
			}
			open={open}
			onOpenChange={(isOpen) => setOpen(isOpen)}
		/>
	);
});

export const UncontrolledConnectModalExample = withProviders(() => {
	const currentAccount = useCurrentAccount();

	return (
		<ConnectModal
			trigger={
				<button disabled={!!currentAccount}> {currentAccount ? 'Connected' : 'Connect'}</button>
			}
		/>
	);
});

function withProviders(Component: React.FunctionComponent<object>) {
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
					<WalletProvider
						stashedWallet={{
							name: 'dApp Kit Docs',
						}}
					>
						<Component />
					</WalletProvider>
				</SuiClientProvider>
			</QueryClientProvider>
		);
	};
}
