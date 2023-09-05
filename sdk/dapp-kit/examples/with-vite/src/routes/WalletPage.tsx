// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallet, useConnectWallet, useDisconnectWallet } from '@mysten/dapp-kit';

export function WalletPage() {
	const { wallets, currentWallet, currentAccount } = useWallet();
	const { mutate: connectWallet, isSuccess, isLoading } = useConnectWallet();
	const { mutate: disconnectWallet } = useDisconnectWallet();

	return (
		<>
			<div
				onClick={() =>
					connectWallet(
						{ walletName: 'Sui Wallet' },
						{
							onSuccess: (accounts) => console.log('ACC', accounts),
							onError: (error) => console.log(error),
						},
					)
				}
			>
				hello
				<div>Connected wallet: {currentWallet?.name}</div>
				<div>Active account address: {currentAccount?.address}</div>
			</div>
			<button onClick={() => disconnectWallet(undefined, {})}>Disconnect</button>
		</>
	);
}
