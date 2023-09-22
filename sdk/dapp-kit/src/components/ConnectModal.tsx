// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import type { ReactNode } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { WalletList } from './WalletList.js';
import { useConnectWallet } from '../hooks/wallet/useConnectWallet.js';
import * as styles from './ConnectButton.css.js';
import { WhatIsAWallet } from './connect-modal/WhatIsAWallet.js';
import { GettingStarted } from './connect-modal/GettingStarted.js';
import { assertUnreachable } from '../utils/assertUnreachable.js';
import { ConnectionStatus } from './connect-modal/ConnectionStatus.js';
import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { CloseIcon } from './icons/CloseIcon.js';

type ConnectModalView = 'getting-started' | 'what-is-a-wallet' | 'connection-status';

type ConnectModalProps = {
	triggerButton: ReactNode;
};

export function ConnectModal({ triggerButton }: ConnectModalProps) {
	const [isConnectModalOpen, setConnectModalOpen] = useState(false);
	const [selectedView, setSelectedView] = useState<ConnectModalView>('what-is-a-wallet');
	const [selectedWallet, setSelectedWallet] = useState<WalletWithRequiredFeatures>();
	const { mutate, isError } = useConnectWallet();

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

	return (
		<Dialog.Root open={isConnectModalOpen} onOpenChange={onOpenChange}>
			<Dialog.Trigger>{triggerButton}</Dialog.Trigger>
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
					<Dialog.Close aria-label="Close">
						<CloseIcon />
					</Dialog.Close>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
