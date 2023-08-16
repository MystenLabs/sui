// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { CheckFill16, SocialGoogle24 } from '@mysten/icons';

import * as ToggleGroup from '@radix-ui/react-toggle-group';
import cn from 'classnames';
import { AccountItem } from './AccountItem';
import { type SerializedUIAccount } from '_src/background/accounts/Account';

type AccountMultiSelectItemProps = {
	account: SerializedUIAccount;
	state?: 'selected' | 'disabled';
};

export function AccountMultiSelectItem({ account, state, ...props }: AccountMultiSelectItemProps) {
	const { data: domainName } = useResolveSuiNSName(account.address);
	return (
		<ToggleGroup.Item asChild value={account.id}>
			<AccountItem
				// TODO: nickname
				name={domainName || ''}
				/* todo: replace this with a real icon */
				icon={<SocialGoogle24 className="h-4 w-4" />}
				address={account.address}
				disabled={state === 'disabled'}
				selected={state === 'selected'}
				after={
					<div
						className={cn(`flex items-center justify-center ml-auto text-hero/10`, {
							'text-success': state === 'selected',
						})}
					>
						<CheckFill16 className={cn('h-4 w-4', { 'opacity-50': state === 'disabled' })} />
					</div>
				}
			/>
		</ToggleGroup.Item>
	);
}
