// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js/utils';

import { AccountIcon } from './AccountIcon';
import { AccountItem } from './AccountItem';
import { LockUnlockButton } from './LockUnlockButton';
import { useUnlockAccount } from './UnlockAccountContext';
import { useActiveAccount } from '../../hooks/useActiveAccount';
import { type SerializedUIAccount } from '_src/background/accounts/Account';

type AccountListItemProps = {
	account: SerializedUIAccount;
	editable?: boolean;
	selected?: boolean;
};

export function AccountListItem({ account }: AccountListItemProps) {
	const activeAccount = useActiveAccount();
	const { data: domainName } = useResolveSuiNSName(account?.address);
	const { unlockAccount, lockAccount, isLoading } = useUnlockAccount();

	return (
		<AccountItem
			icon={<AccountIcon account={account} />}
			name={account.nickname || domainName || formatAddress(account.address)}
			isActiveAccount={account.address === activeAccount?.address}
			after={
				<div className="ml-auto">
					<div className="flex items-center justify-center">
						<LockUnlockButton
							isLocked={account.isLocked}
							isLoading={isLoading}
							onClick={(e) => {
								// prevent the account from being selected when clicking the lock button
								e.stopPropagation();
								if (account.isLocked) {
									unlockAccount(account);
								} else {
									lockAccount(account);
								}
							}}
						/>
					</div>
				</div>
			}
			address={account.address}
		/>
	);
}
