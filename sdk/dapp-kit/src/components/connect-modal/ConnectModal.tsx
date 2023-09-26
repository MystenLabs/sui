// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import * as Dialog from '@radix-ui/react-dialog';
import clsx from 'clsx';
import { useState } from 'react';
import type { ReactNode } from 'react';

import { useConnectWallet } from '../../hooks/wallet/useConnectWallet.js';
import * as styles from './ConnectModal.css.js';
import { ConnectionStatus } from './views/ConnectionStatus.js';
import { GettingStarted } from './views/GettingStarted.js';
import { WhatIsAWallet } from './views/WhatIsAWallet.js';
import { WalletList } from './wallet-list/WalletList.js';
import { BackIcon } from '../icons/BackIcon.js';
import { CloseIcon } from '../icons/CloseIcon.js';

type ConnectModalView = 'getting-started' | 'what-is-a-wallet' | 'connection-status';

type ConnectModalProps = {
	trigger: ReactNode;
};

export function ConnectModal({ trigger }: ConnectModalProps) {
	const [isConnectModalOpen, setConnectModalOpen] = useState(false);
	const [currentView, setCurrentView] = useState<ConnectModalView>();
	const [selectedWallet, setSelectedWallet] = useState<WalletWithRequiredFeatures>();
	const { mutate, isError } = useConnectWallet();

	const connectWallet = (wallet: WalletWithRequiredFeatures) => {
		// Set a quick timeout here so we don't flash the connection status UI
		// when the user has previously authorized a set of wallet accounts.
		setTimeout(() => setCurrentView('connection-status'), 100);
		mutate({ wallet }, { onSuccess: () => setConnectModalOpen(false) });
	};

	const resetSelection = () => {
		setSelectedWallet(undefined);
		setCurrentView(undefined);
	};

	const onOpenChange = (open: boolean) => {
		if (!open) {
			resetSelection();
		}
		setConnectModalOpen(open);
	};

	let modalContent: ReactNode | undefined;
	switch (currentView) {
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
			modalContent = <WhatIsAWallet />;
	}

	return (
		<Dialog.Root open={isConnectModalOpen} onOpenChange={onOpenChange}>
			<Dialog.Trigger className={styles.triggerButton}>{trigger}</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay className={styles.overlay} />
				<Dialog.Content className={styles.content} aria-describedby={undefined}>
					<div
						className={clsx(styles.walletListContainer, {
							[styles.selectedWalletListContainer]: !!currentView,
						})}
					>
						<Dialog.Title className={styles.title}>Connect a Wallet</Dialog.Title>
						<WalletList
							selectedWalletName={selectedWallet?.name}
							onPlaceholderClick={() => setCurrentView('getting-started')}
							onSelect={(wallet) => {
								setSelectedWallet(wallet);
								connectWallet(wallet);
							}}
						/>
					</div>
					<div
						className={clsx(styles.viewContainer, {
							[styles.selectedViewContainer]: !!currentView,
						})}
					>
						<button
							className={styles.backButton}
							type="button"
							aria-label="Back"
							onClick={() => resetSelection()}
						>
							<BackIcon />
						</button>
						{modalContent}
					</div>
					<button
						className={styles.whatIsAWalletButton}
						type="button"
						onClick={() => setCurrentView('what-is-a-wallet')}
					>
						What is a Wallet?
					</button>
					<Dialog.Close className={styles.closeButton} aria-label="Close">
						<CloseIcon />
					</Dialog.Close>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
