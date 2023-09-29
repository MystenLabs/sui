// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js/utils';
import type { WalletAccount } from '@mysten/wallet-standard';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';

import { useAccounts } from '../hooks/wallet/useAccounts.js';
import { useDisconnectWallet } from '../hooks/wallet/useDisconnectWallet.js';
import { useSwitchAccount } from '../hooks/wallet/useSwitchAccount.js';
import * as styles from './AccountDropdownMenu.css.js';
import { CheckIcon } from './icons/CheckIcon.js';
import { ChevronIcon } from './icons/ChevronIcon.js';

type AccountDropdownMenuProps = {
	currentAccount: WalletAccount;
};

export function AccountDropdownMenu({ currentAccount }: AccountDropdownMenuProps) {
	const { mutate: disconnectWallet } = useDisconnectWallet();
	const { mutate: switchAccount } = useSwitchAccount();
	const accounts = useAccounts();

	return (
		<DropdownMenu.Root modal={false}>
			<DropdownMenu.Trigger className={styles.triggerButton}>
				{formatAddress(currentAccount.address)}
				<ChevronIcon />
			</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content className={styles.menuContent}>
					{accounts.map((account) => (
						<DropdownMenu.Item key={account.address} asChild>
							<button
								type="button"
								className={styles.switchAccountButton}
								onClick={() => switchAccount({ account })}
							>
								{formatAddress(account.address)}
								{currentAccount.address === account.address ? <CheckIcon /> : null}
							</button>
						</DropdownMenu.Item>
					))}
					<DropdownMenu.Separator className={styles.separator} />
					<DropdownMenu.Item asChild>
						<button
							className={styles.disconnectButton}
							type="button"
							onClick={() => disconnectWallet()}
						>
							Disconnect
						</button>
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	);
}
