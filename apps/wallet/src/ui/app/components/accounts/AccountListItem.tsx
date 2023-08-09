// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js/utils';

import { type ReactNode } from 'react';
import { AccountItem } from './AccountItem';
import { LockUnlockButton } from './LockUnlockButton';
import { useActiveAddress } from '../../hooks';
import { useAccounts } from '../../hooks/useAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';

type AccountListItemProps = {
	address: string;
	icon?: ReactNode;
	editable?: boolean;
	isLocked?: boolean;
	selected?: boolean;
};

export function AccountListItem({ address, icon, selected, isLocked }: AccountListItemProps) {
	const activeAddress = useActiveAddress();
	const { data: accounts } = useAccounts();
	const account = accounts?.find((account) => account.address === address);
	const { data: domainName } = useResolveSuiNSName(address);
	const { unlockAccountSourceOrAccount, lockAccountSourceOrAccount } = useBackgroundClient();

	if (!account) return null;

	return (
		<AccountItem
			icon={icon}
			name={account.nickname || domainName || formatAddress(address)}
			selected={address === activeAddress}
			after={
				<div className="ml-auto">
					<div className="flex items-center justify-center">
						<LockUnlockButton
							isLocked={account.isLocked}
							onClick={() => {
								if (isLocked) {
									unlockAccountSourceOrAccount({ id: account.id });
								} else {
									lockAccountSourceOrAccount({ id: account.id });
								}
							}}
						/>
					</div>
				</div>
			}
			address={address}
		/>
	);
}
