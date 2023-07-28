// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { CheckFill16, SocialGoogle24 } from '@mysten/icons';

import * as ToggleGroup from '@radix-ui/react-toggle-group';
import classNames from 'classnames';
import { AccountItem } from './AccountItem';

type AccountMultiSelectItem = {
	address: string;
	name?: string;
	disabled?: boolean;
	selected: boolean;
};

type AccountMultiSelectItemProps = {
	address: string;
	name?: string;
	disabled?: boolean;
	selected: boolean;
};

export function AccountMultiSelectItem({
	address,
	selected,
	disabled,
	...props
}: AccountMultiSelectItemProps) {
	const { data: domainName } = useResolveSuiNSName(address);

	return (
		<ToggleGroup.Item asChild value={address}>
			<AccountItem
				name={domainName}
				icon={<SocialGoogle24 className="h-4 w-4" />}
				address={address}
				selected={selected && !disabled}
				disabled={disabled}
				after={
					<div
						className={`flex items-center justify-center ml-auto ${
							selected && !disabled ? 'text-success' : 'text-hero/10'
						}`}
					>
						<CheckFill16 className={classNames('h-4 w-4', { 'opacity-50': disabled })} />
					</div>
				}
				{...props}
			/>
		</ToggleGroup.Item>
	);
}
