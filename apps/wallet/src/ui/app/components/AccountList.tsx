// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AccountListItem, type AccountItemProps } from './AccountListItem';
import { useAccounts } from '../hooks/useAccounts';

export type AccountListProps = {
	onAccountSelected: AccountItemProps['onAccountSelected'];
};

export function AccountList({ onAccountSelected }: AccountListProps) {
	const allAccounts = useAccounts();
	return (
		<ul className="list-none m-0 px-0 py-1.25 flex flex-col items-stretch">
			{allAccounts.map((account) => (
				<AccountListItem
					account={account}
					key={account.address}
					onAccountSelected={onAccountSelected}
				/>
			))}
		</ul>
	);
}
