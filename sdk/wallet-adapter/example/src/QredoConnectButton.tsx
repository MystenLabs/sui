// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit, type WalletWithFeatures } from '@mysten/wallet-kit';

type QredoConnectInput = {
	service: string;
	apiUrl: string;
	token: string;
	organization: string;
};
type QredoConnectFeature = {
	'qredo:connect': {
		version: '0.0.1';
		qredoConnect: (input: QredoConnectInput) => Promise<void>;
	};
};
type QredoConnectWallet = WalletWithFeatures<Partial<QredoConnectFeature>>;

export function QredoConnectButton() {
	const { wallets } = useWalletKit();
	const selectedWallet = wallets.filter(
		(aWallet) =>
			'wallet' in aWallet && !!(aWallet.wallet as QredoConnectWallet).features['qredo:connect'],
	)[0];
	if (!selectedWallet || !('wallet' in selectedWallet)) {
		return (
			// eslint-disable-next-line react/jsx-no-target-blank
			<a
				href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil"
				target="_blank"
			>
				Install Sui Wallet to stake SUI
			</a>
		);
	}
	const qredoConnectWallet = selectedWallet.wallet as QredoConnectWallet;
	return (
		<button
			onClick={async () => {
				try {
					await qredoConnectWallet.features['qredo:connect']?.qredoConnect({
						service: 'qredo-testing',
						apiUrl: 'http://localhost:8080/connect/sui',
						token: 'aToken',
						organization: 'org1',
					});
				} catch (e) {
					console.log(e);
				}
			}}
		>
			Connect
		</button>
	);
}
