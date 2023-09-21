// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import type { ReactNode } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { useCurrentAccount } from '../hooks/wallet/useCurrentAccount.js';
import { WalletList } from './WalletList.js';
import { useConnectWallet } from '../hooks/wallet/useConnectWallet.js';
import * as styles from './ConnectButton.css.js';
import { WhatIsAWallet } from './connect-modal/WhatIsAWallet.js';
import { GettingStarted } from './connect-modal/GettingStarted.js';
import { assertUnreachable } from '../utils/assertUnreachable.js';
import { ConnectionStatus } from './connect-modal/ConnectionStatus.js';
import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';

type ConnectButtonProps = {
	connectText?: ReactNode;
};

type ConnectModalView = 'getting-started' | 'what-is-a-wallet' | 'connection-status';

export function ConnectButton({ connectText = 'Connect Wallet' }: ConnectButtonProps) {
	const [isConnectModalOpen, setConnectModalOpen] = useState(false);

	const [selectedView, setSelectedView] = useState<ConnectModalView>('what-is-a-wallet');
	const [selectedWallet, setSelectedWallet] = useState<WalletWithRequiredFeatures>();

	const { mutate, isError } = useConnectWallet();
	const currentAccount = useCurrentAccount();

	const connectWallet = (wallet: WalletWithRequiredFeatures) => {
		setSelectedView('connection-status');
		mutate(
			{ wallet },
			{
				onSuccess: () => setConnectModalOpen(false),
			},
		);
	};

	const onOpenChange = (open: boolean) => {
		if (!open) {
			setSelectedWallet(undefined);
			setSelectedView('what-is-a-wallet');
		}
		setConnectModalOpen(open);
	};

	let modalContent: ReactNode | undefined;
	switch (selectedView) {
		case 'what-is-a-wallet':
			modalContent = <WhatIsAWallet />;
			break;
		case 'getting-started':
			modalContent = <GettingStarted />;
			break;
		case 'connection-status':
			modalContent = selectedWallet ? (
				<ConnectionStatus
					selectedWallet={selectedWallet}
					hadConnectionError={isError}
					onRetryConnection={connectWallet}
				/>
			) : null;
			break;
		default:
			assertUnreachable(selectedView);
	}

	return currentAccount ? (
		<div>dropdown menu</div>
	) : (
		<Dialog.Root open={isConnectModalOpen} onOpenChange={onOpenChange}>
			<Dialog.Trigger>{connectText}</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay className={styles.modalOverlay} />
				<Dialog.Content className={styles.modalContent} aria-describedby={undefined}>
					<div className={styles.walletListContainer}>
						<Dialog.Title>Connect a Wallet</Dialog.Title>
						<WalletList
							onPlaceholderClick={() => setSelectedView('getting-started')}
							selectedWalletName={selectedWallet?.name}
							onSelect={(wallet) => {
								setSelectedWallet(wallet);
								connectWallet(wallet);
							}}
						/>
					</div>
					{modalContent}
					<Dialog.Close />
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
