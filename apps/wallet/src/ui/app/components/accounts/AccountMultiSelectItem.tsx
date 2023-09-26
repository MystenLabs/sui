// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { CheckFill16 } from '@mysten/icons';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import cn from 'classnames';

import { AccountIcon } from './AccountIcon';
import { AccountItem } from './AccountItem';

type AccountMultiSelectItemProps = {
	account: SerializedUIAccount;
	state?: 'selected' | 'disabled';
};

export function AccountMultiSelectItem({ account, state }: AccountMultiSelectItemProps) {
	return (
		<ToggleGroup.Item asChild value={account.id}>
			<AccountItem
				accountID={account.id}
				selected={state === 'selected'}
				disabled={state === 'disabled'}
				icon={<AccountIcon account={account} />}
				after={
					<div
						className={cn(`flex items-center justify-center ml-auto text-hero/10`, {
							'text-success': state === 'selected',
						})}
					>
						<CheckFill16 className={cn('h-4 w-4', { 'opacity-50': state === 'disabled' })} />
					</div>
				}
				hideCopy
				hideExplorerLink
			/>
		</ToggleGroup.Item>
	);
}
