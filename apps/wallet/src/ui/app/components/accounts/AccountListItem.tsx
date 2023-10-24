// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';

import { useActiveAccount } from '../../hooks/useActiveAccount';
import { AccountIcon } from './AccountIcon';
import { AccountItem } from './AccountItem';
import { LockUnlockButton } from './LockUnlockButton';
import { useUnlockAccount } from './UnlockAccountContext';

type AccountListItemProps = {
	account: SerializedUIAccount;
	editable?: boolean;
	showLock?: boolean;
	hideCopy?: boolean;
	hideExplorerLink?: boolean;
};

export function AccountListItem({
	account,
	editable,
	showLock,
	hideCopy,
	hideExplorerLink,
}: AccountListItemProps) {
	const activeAccount = useActiveAccount();
	const { unlockAccount, lockAccount, isPending, accountToUnlock } = useUnlockAccount();

	return (
		<AccountItem
			icon={<AccountIcon account={account} />}
			isActiveAccount={account.address === activeAccount?.address}
			after={
				showLock ? (
					<div className="ml-auto">
						<div className="flex items-center justify-center">
							<LockUnlockButton
								isLocked={account.isLocked}
								isLoading={isPending && accountToUnlock?.id === account.id}
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
				) : null
			}
			accountID={account.id}
			editable={editable}
			hideCopy={hideCopy}
			hideExplorerLink={hideExplorerLink}
		/>
	);
}
