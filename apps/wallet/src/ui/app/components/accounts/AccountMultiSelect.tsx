// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import * as ToggleGroup from '@radix-ui/react-toggle-group';

import { AccountMultiSelectItem } from './AccountMultiSelectItem';
import { type SerializedAccount } from '_src/background/keyring/Account';

type AccountMultiSelectProps = {
	accounts: SerializedAccount[];
	value: string[];
	onChange: (value: string[]) => void;
};

export function AccountMultiSelect({ accounts, value, onChange }: AccountMultiSelectProps) {
	return (
		<ToggleGroup.Root
			value={value}
			onValueChange={onChange}
			type="multiple"
			className="flex flex-col gap-3"
		>
			{accounts.map((account) => (
				<AccountMultiSelectItem
					key={account.address}
					address={account.address}
					selected={value?.includes(account.address) ?? false}
				/>
			))}
		</ToggleGroup.Root>
	);
}
