// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { AccountGroup } from './AccountGroup';

import Overlay from '../../../components/overlay';
import { useAccounts } from '../../../hooks/useAccounts';
import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';

export function ManageAccountsPage() {
	const { data: accounts } = useAccounts();

	const navigate = useNavigate();
	const groupedAccounts = useMemo(() => {
		return (accounts ?? []).reduce(
			(acc, account) => {
				if (!acc[account.type]) {
					acc[account.type] = [];
				}
				acc[account.type].push(account);
				return acc;
			},
			{} as Record<AccountType, SerializedUIAccount[]>,
		);
	}, [accounts]);

	return (
		<Overlay showModal title="Manage Accounts" closeOverlay={() => navigate('/home')}>
			<div className="flex flex-col gap-4 flex-1">
				{Object.entries(groupedAccounts).map(([type, accounts]) => {
					let accountSource;
					// todo: is there a better way???
					if (isMnemonicSerializedUiAccount(accounts[0])) accountSource = accounts[0].sourceID;
					return (
						<AccountGroup
							key={type}
							accounts={accounts ?? []}
							accountSource={accountSource}
							type={type as AccountType}
						/>
					);
				})}
			</div>
		</Overlay>
	);
}
