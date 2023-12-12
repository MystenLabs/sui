// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AccountItemApproveConnection } from '_components/accounts/AccountItemApproveConnection';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useState } from 'react';

import { Button } from '../../shared/ButtonUI';

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
				<ToggleGroup.Item key={account.id} asChild value={account.id}>
					<div>
						<AccountItemApproveConnection
							disabled={account.isLocked}
							account={account}
							selected={selectedAccountIDs.includes(account.id)}
						/>
					</div>
				</ToggleGroup.Item>
			))}
		</ToggleGroup.Root>
	);
}

export function AccountMultiSelectWithControls({
	selectedAccountIDs: selectedAccountsFromProps,
	accounts,
	onChange: onChangeFromProps,
}: AccountMultiSelectProps) {
	const [selectedAccountIds, setSelectedAccountsIds] = useState(selectedAccountsFromProps);
	const onChange = (value: string[]) => {
		setSelectedAccountsIds(value);
		onChangeFromProps(value);
	};
	return (
		<div className="flex flex-col gap-3 [&>button]:border-none">
			<AccountMultiSelect
				selectedAccountIDs={selectedAccountIds}
				accounts={accounts}
				onChange={onChange}
			/>

			{accounts.length > 1 ? (
				<Button
					onClick={() => {
						if (selectedAccountIds.length < accounts.length) {
							// select all accounts if not all are selected
							onChange(accounts.map((account) => account.id));
						} else {
							// deselect all accounts
							onChange([]);
						}
					}}
					variant="outline"
					size="xs"
					text={
						selectedAccountIds.length < accounts.length
							? 'Select All Accounts'
							: 'Deselect All Accounts'
					}
				/>
			) : null}
		</div>
	);
}
