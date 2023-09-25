// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import { useCurrentAccount } from '../hooks/wallet/useCurrentAccount.js';
import { useAccounts } from '../hooks/wallet/useAccounts.js';
import { formatAddress } from '@mysten/sui.js/utils';
import { useDisconnectWallet } from '../hooks/wallet/useDisconnectWallet.js';
import { useSwitchAccount } from '../hooks/wallet/useSwitchAccount.js';

export function AccountDropdownMenu() {
	const { mutate: disconnectWallet } = useDisconnectWallet();
	const { mutate: switchAccount } = useSwitchAccount();
	const currentAccount = useCurrentAccount();
	const accounts = useAccounts();

	return currentAccount ? (
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>{formatAddress(currentAccount?.address!)}</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content>
					{accounts.map((account) => (
						<DropdownMenu.Item key={account.address} asChild>
							<button type="button" onClick={() => switchAccount({ account })}>
								{formatAddress(account.address)}
							</button>
						</DropdownMenu.Item>
					))}
					<DropdownMenu.Separator />
					<DropdownMenu.Item>
						<button type="button" onClick={() => disconnectWallet()}>
							Disconnect
						</button>
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	) : null;
}
