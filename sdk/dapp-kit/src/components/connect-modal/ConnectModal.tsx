// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import * as Dialog from '@radix-ui/react-dialog';
import clsx from 'clsx';
import { useState } from 'react';
import type { ReactNode } from 'react';

import { useConnectWallet } from '../../hooks/wallet/useConnectWallet.js';
import { getWalletUniqueIdentifier } from '../../utils/walletUtils.js';
import { BackIcon } from '../icons/BackIcon.js';
import { CloseIcon } from '../icons/CloseIcon.js';
import { StyleMarker } from '../styling/StyleMarker.js';
import { Heading } from '../ui/Heading.js';
import { IconButton } from '../ui/IconButton.js';
import * as styles from './ConnectModal.css.js';
import { ConnectionStatus } from './views/ConnectionStatus.js';
import { GettingStarted } from './views/GettingStarted.js';
import { WhatIsAWallet } from './views/WhatIsAWallet.js';
import { WalletList } from './wallet-list/WalletList.js';

type ConnectModalView = 'getting-started' | 'what-is-a-wallet' | 'connection-status';

type ControlledModalProps = {
	/** The controlled open state of the dialog. */
	open: boolean;

	/** Event handler called when the open state of the dialog changes. */
	onOpenChange: (open: boolean) => void;

	defaultOpen?: never;
};

type UncontrolledModalProps = {
	open?: never;

	onOpenChange?: never;

	/** The open state of the dialog when it is initially rendered. Use when you do not need to control its open state. */
	defaultOpen?: boolean;
};

type ConnectModalProps = {
	/** The trigger button that opens the dialog. */
	trigger: NonNullable<ReactNode>;
} & (ControlledModalProps | UncontrolledModalProps);

export function ConnectModal({ trigger, open, defaultOpen, onOpenChange }: ConnectModalProps) {
	const [isModalOpen, setModalOpen] = useState(open ?? defaultOpen);
	const [currentView, setCurrentView] = useState<ConnectModalView>();
	const [selectedWallet, setSelectedWallet] = useState<WalletWithRequiredFeatures>();
	const { mutate, isError } = useConnectWallet();

	const resetSelection = () => {
		setSelectedWallet(undefined);
		setCurrentView(undefined);
	};

	const handleOpenChange = (open: boolean) => {
		if (!open) {
			resetSelection();
		}
		setModalOpen(open);
		onOpenChange?.(open);
	};

	const connectWallet = (wallet: WalletWithRequiredFeatures) => {
		setCurrentView('connection-status');
		mutate(
			{ wallet },
			{
				onSuccess: () => handleOpenChange(false),
			},
		);
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
			modalContent = (
				<ConnectionStatus
					selectedWallet={selectedWallet!}
					hadConnectionError={isError}
					onRetryConnection={connectWallet}
				/>
			);
			break;
		default:
			modalContent = <WhatIsAWallet />;
	}

	return (
		<Dialog.Root open={open ?? isModalOpen} onOpenChange={handleOpenChange}>
			<Dialog.Trigger asChild>{trigger}</Dialog.Trigger>
			<Dialog.Portal>
				<StyleMarker>
					<Dialog.Overlay className={styles.overlay}>
						<Dialog.Content className={styles.content} aria-describedby={undefined}>
							<div
								className={clsx(styles.walletListContainer, {
									[styles.walletListContainerWithViewSelected]: !!currentView,
								})}
							>
								<div className={styles.walletListContent}>
									<Dialog.Title className={styles.title} asChild>
										<Heading as="h2">Connect a Wallet</Heading>
									</Dialog.Title>
									<WalletList
										selectedWalletName={getWalletUniqueIdentifier(selectedWallet)}
										onPlaceholderClick={() => setCurrentView('getting-started')}
										onSelect={(wallet) => {
											if (
												getWalletUniqueIdentifier(selectedWallet) !==
												getWalletUniqueIdentifier(wallet)
											) {
												setSelectedWallet(wallet);
												connectWallet(wallet);
											}
										}}
									/>
								</div>
								<button
									className={styles.whatIsAWalletButton}
									onClick={() => setCurrentView('what-is-a-wallet')}
									type="button"
								>
									What is a Wallet?
								</button>
							</div>
							<div
								className={clsx(styles.viewContainer, {
									[styles.selectedViewContainer]: !!currentView,
								})}
							>
								<div className={styles.backButtonContainer}>
									<IconButton type="button" aria-label="Back" onClick={() => resetSelection()}>
										<BackIcon />
									</IconButton>
								</div>
								{modalContent}
							</div>
							<Dialog.Close className={styles.closeButtonContainer} asChild>
								<IconButton type="button" aria-label="Close">
									<CloseIcon />
								</IconButton>
							</Dialog.Close>
						</Dialog.Content>
					</Dialog.Overlay>
				</StyleMarker>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
