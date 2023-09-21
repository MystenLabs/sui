// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
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

type ConnectButtonProps = {
	connectText?: ReactNode;
};

type ConnectModalView = 'getting-started' | 'what-is-a-wallet' | 'connection-status';

export function ConnectButton({ connectText = 'Connect Wallet' }: ConnectButtonProps) {
	const [isConnectModalOpen, setConnectModalOpen] = useState(false);
	const [selectedView, setSelectedView] = useState<ConnectModalView>('what-is-a-wallet');
	const { mutate: connectWallet, ...rest } = useConnectWallet();
	console.log(rest.variables);
	const currentAccount = useCurrentAccount();

	let modalContent: ReactNode | undefined;
	switch (selectedView) {
		case 'what-is-a-wallet':
			modalContent = <WhatIsAWallet />;
			break;
		case 'getting-started':
			modalContent = <GettingStarted />;
			break;
		case 'connection-status':
			modalContent = <div>hi</div>;
			break;
		default:
			assertUnreachable(selectedView);
	}

	return currentAccount ? (
		<DropdownMenu.Root>
			<DropdownMenu.Trigger asChild>
				<button type="button"></button>
			</DropdownMenu.Trigger>
		</DropdownMenu.Root>
	) : (
		<Dialog.Root open={isConnectModalOpen} onOpenChange={setConnectModalOpen}>
			<Dialog.Trigger>{connectText}</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay className={styles.modalOverlay} />
				<Dialog.Content className={styles.modalContent} aria-describedby={undefined}>
					<div className={styles.walletListContainer}>
						<Dialog.Title>Connect a Wallet</Dialog.Title>
						<WalletList
							onPlaceholderClick={() => setSelectedView('getting-started')}
							onSelect={(wallet) => {
								connectWallet({ wallet }, { onSuccess: () => setConnectModalOpen(false) });
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
