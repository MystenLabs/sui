// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallet, useConnectWallet } from '@mysten/dapp-kit';

export function WalletPage() {
	const { wallets } = useWallet();
	const { mutate: connectWallet, isSuccess, isLoading } = useConnectWallet();

	console.log('test', isLoading, isSuccess, isLoading);
	console.log('test', wallets);

	return (
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
		</div>
	);
}
