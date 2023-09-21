// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';
import { isZkLoginAccountSerializedUI } from '_src/background/accounts/zklogin/ZkLoginAccount';

function getKey(account: SerializedUIAccount): string {
	if (isMnemonicSerializedUiAccount(account)) return account.sourceID;
	if (isZkLoginAccountSerializedUI(account)) return account.provider;
	return account.type;
}

export const defaultSortOrder: AccountType[] = [
	'zkLogin',
	'mnemonic-derived',
	'imported',
	'ledger',
	'qredo',
];

export function getAccountBackgroundByType(account: SerializedUIAccount) {
	if (!isZkLoginAccountSerializedUI(account)) return 'bg-gradients-graph-cards';
	switch (account.provider) {
		case 'google':
			return 'bg-google bg-no-repeat bg-cover';
		case 'twitch':
			return 'bg-twitch-image bg-no-repeat bg-cover';
		default:
			return `bg-gradients-graph-cards`;
	}
}

export function groupByType(accounts: SerializedUIAccount[]) {
	return accounts.reduce(
		(acc, account) => {
			const byType = acc[account.type] || (acc[account.type] = {});
			const key = getKey(account);
			(byType[key] || (byType[key] = [])).push(account);
			return acc;
		},
		defaultSortOrder.reduce(
			(acc, type) => {
				acc[type] = {};
				return acc;
			},
			{} as Record<AccountType, Record<string, SerializedUIAccount[]>>,
		),
	);
}
