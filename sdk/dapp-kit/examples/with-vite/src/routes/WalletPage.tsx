// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	useWallet,
	useConnectWallet,
	useDisconnectWallet,
	useSwitchAccount,
} from '@mysten/dapp-kit';
import { Button } from '@/components/ui/button';

export function WalletPage() {
	const { wallets, currentWallet, currentAccount, accounts, connectionStatus } = useWallet();
	const { mutate: connectWallet } = useConnectWallet();
	const { mutate: disconnectWallet } = useDisconnectWallet();
	const { mutate: switchAccount } = useSwitchAccount();

	const isWalletDisconnected = connectionStatus === 'disconnected';
	const isWalletConnecting = connectionStatus === 'connecting';

	return (
		<div className="flex flex-col gap-4">
			<div className="flex gap-4">
				{wallets.length > 0 ? (
					<ul className="flex flex-col gap-3">
						{wallets.map((wallet) => (
							<li key={wallet.name}>
								<Button
									onClick={() => connectWallet({ walletName: wallet.name })}
									disabled={!isWalletDisconnected}
								>
									Connect {wallet.name}
								</Button>
							</li>
						))}
						<li>
							<Button
								onClick={() => disconnectWallet()}
								disabled={isWalletDisconnected || isWalletConnecting}
							>
								Disconnect Wallet
							</Button>
						</li>
					</ul>
				) : (
					<div>You don't have any registered wallets</div>
				)}
			</div>

			<div className="">
				<div className="">Connection status: {connectionStatus}</div>
				<div className="">Connected wallet: {currentWallet?.name ?? 'N/A'}</div>
				<div className="">Current account: {currentAccount?.address ?? 'N/A'}</div>
				{accounts.length > 0 && (
					<ul>
						{accounts.map((account) => (
							<li key={account.address}>
								<Button
									variant="link"
									onClick={() => switchAccount({ accountAddress: account.address })}
								>
									{account.address}
								</Button>
							</li>
						))}
					</ul>
				)}
			</div>
		</div>
	);
}
