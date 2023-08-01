// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js/utils';
import * as ToggleGroup from '@radix-ui/react-toggle-group';

import { AccountMultiSelectItem } from './AccountMultiSelectItem';
import { type SerializedAccount } from '_src/background/keyring/Account';

type AccountMultiSelectProps = {
	accounts: SerializedAccount[];
	selectedAccounts: string[];
	onChange: (value: string[]) => void;
};

export function AccountMultiSelect({
	accounts,
	selectedAccounts,
	onChange,
}: AccountMultiSelectProps) {
	return (
		<ToggleGroup.Root
			value={selectedAccounts}
			onValueChange={onChange}
			type="multiple"
			className="flex flex-col gap-3"
		>
			{accounts.map((account) => (
				<AccountMultiSelectItem
					key={account.address}
					name={formatAddress(account.address)}
					address={account.address}
					state={selectedAccounts?.includes(account.address) ? 'selected' : undefined}
				/>
			))}
		</ToggleGroup.Root>
	);
}
