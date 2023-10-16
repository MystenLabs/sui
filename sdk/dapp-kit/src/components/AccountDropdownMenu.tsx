// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js/utils';
import type { WalletAccount } from '@mysten/wallet-standard';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import clsx from 'clsx';

import { useResolveSuiNSName } from '../hooks/useResolveSuiNSNames.js';
import { useAccounts } from '../hooks/wallet/useAccounts.js';
import { useDisconnectWallet } from '../hooks/wallet/useDisconnectWallet.js';
import { useSwitchAccount } from '../hooks/wallet/useSwitchAccount.js';
import * as styles from './AccountDropdownMenu.css.js';
import { CheckIcon } from './icons/CheckIcon.js';
import { ChevronIcon } from './icons/ChevronIcon.js';
import { StyleMarker } from './styling/StyleMarker.js';
import { Button } from './ui/Button.js';
import { Text } from './ui/Text.js';

type AccountDropdownMenuProps = {
	currentAccount: WalletAccount;
};

export function AccountDropdownMenu({ currentAccount }: AccountDropdownMenuProps) {
	const { mutate: disconnectWallet } = useDisconnectWallet();
	const { mutate: switchAccount } = useSwitchAccount();
	const { data: domain } = useResolveSuiNSName(
		currentAccount.label ? null : currentAccount.address,
	);
	const accounts = useAccounts();

	return (
		<DropdownMenu.Root modal={false}>
			<StyleMarker>
				<DropdownMenu.Trigger asChild>
					<Button size="lg" className={styles.connectedAccount}>
						<Text mono weight="bold">
							{currentAccount.label ?? domain ?? formatAddress(currentAccount.address)}
						</Text>
						<ChevronIcon />
					</Button>
				</DropdownMenu.Trigger>
			</StyleMarker>
			<DropdownMenu.Portal>
				<StyleMarker>
					<DropdownMenu.Content className={styles.menuContent}>
						{accounts.map((account) => (
							<DropdownMenu.Item
								key={account.address}
								className={clsx(styles.menuItem, styles.switchAccountMenuItem)}
								onSelect={() => switchAccount({ account })}
							>
								<Text mono>{account.label ?? formatAddress(account.address)}</Text>
								{currentAccount.address === account.address ? <CheckIcon /> : null}
							</DropdownMenu.Item>
						))}
						<DropdownMenu.Separator className={styles.separator} />
						<DropdownMenu.Item
							className={clsx(styles.menuItem)}
							onSelect={() => disconnectWallet()}
						>
							Disconnect
						</DropdownMenu.Item>
					</DropdownMenu.Content>
				</StyleMarker>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	);
}
