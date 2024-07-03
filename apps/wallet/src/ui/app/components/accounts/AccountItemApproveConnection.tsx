// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '_app/hooks/useAppResolveSuinsName';
import { AccountIcon } from '_components/accounts/AccountIcon';
import { LockUnlockButton } from '_components/accounts/LockUnlockButton';
import { useUnlockAccount } from '_components/accounts/UnlockAccountContext';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { CheckFill16 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui/utils';
import clsx from 'clsx';

interface Props {
	account: SerializedUIAccount;
	disabled: boolean;
	selected?: boolean;
	showLock?: boolean;
}

export function AccountItemApproveConnection({ account, selected, disabled, showLock }: Props) {
	const domainName = useResolveSuiNSName(account?.address);
	const accountName = account?.nickname ?? domainName ?? formatAddress(account?.address || '');
	const { unlockAccount, lockAccount, isPending, accountToUnlock } = useUnlockAccount();

	return (
		<div
			className={clsx(
				'group rounded-xl px-4 py-3 border border-solid border-hero/10 cursor-pointer',
				'flex items-center justify-start gap-3',
				selected ? 'shadow-card-soft bg-white/80' : 'bg-white/40 hover:bg-white/60',
				disabled ? '!bg-hero-darkest/10 border-transparent' : 'hover:shadow',
			)}
		>
			<AccountIcon account={account} />

			<div className="flex flex-col gap-1 overflow-hidden items-start">
				<div
					className={clsx(
						'truncate font-sans font-semibold text-body group-hover:text-steel-darker',
						selected ? 'text-steel-darker' : 'text-steel-dark',
						disabled && '!text-steel-darker',
					)}
				>
					{accountName}
				</div>

				<div
					className={clsx(
						'truncate font-mono text-subtitle font-semibold',
						disabled ? 'text-steel-darker' : 'text-steel group-hover:text-steel-dark',
					)}
				>
					{formatAddress(account.address)}
				</div>
			</div>

			<div className="ml-auto flex gap-4">
				{showLock ? (
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
				) : null}

				<div
					className={clsx(`flex items-center justify-center ml-auto text-hero/10`, {
						'text-success': selected,
					})}
				>
					<CheckFill16 className={clsx('h-4 w-4', { 'opacity-50': !selected })} />
				</div>
			</div>
		</div>
	);
}
