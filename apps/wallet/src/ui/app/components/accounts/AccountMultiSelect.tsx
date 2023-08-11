// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as ToggleGroup from '@radix-ui/react-toggle-group';

import { AccountMultiSelectItem } from './AccountMultiSelectItem';
import { type SerializedUIAccount } from '_src/background/accounts/Account';

type AccountMultiSelectProps = {
	accounts: SerializedUIAccount[];
	selectedAccountIDs: string[];
	onChange: (value: string[]) => void;
};

export function AccountMultiSelect({
	accounts,
	selectedAccountIDs,
	onChange,
}: AccountMultiSelectProps) {
	return (
		<ToggleGroup.Root
			value={selectedAccountIDs}
			onValueChange={onChange}
			type="multiple"
			className="flex flex-col gap-3"
		>
			{accounts.map((account) => (
				<AccountMultiSelectItem
					key={account.id}
					account={account}
					state={account.selected ? 'selected' : undefined}
				/>
			))}
		</ToggleGroup.Root>
	);
}
