// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallet, useConnectWallet, useDisconnectWallet } from '@mysten/dapp-kit';

export function WalletPage() {
	const { wallets, currentWallet, currentAccount } = useWallet();
	const { mutate: connectWallet } = useConnectWallet();
	const { mutate: disconnectWallet } = useDisconnectWallet();

	return (
		<div>
			<button
				onClick={() => {
					connectWallet({ walletName: 'Sui Wallet' });
				}}
			>
				Connect Wallet
			</button>
			<button onClick={() => disconnectWallet()}>Disconnect Wallet</button>
		</div>
	);
}
