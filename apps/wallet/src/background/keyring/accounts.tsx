// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFromLocalStorage, setToLocalStorage } from '../storage-utils';

export type AccountPublicInfo = {
	// base64 of the public key - always ed25519 key
	publicKey: string | null;
};

const STORE_KEY = 'accountsPublicInfo';

type AccountsPublicInfo = Record<string, AccountPublicInfo>;

export async function getStoredAccountsPublicInfo(): Promise<AccountsPublicInfo> {
	return (await getFromLocalStorage(STORE_KEY, {})) || {};
}

export function storeAccountsPublicInfo(accountsPublicInfo: AccountsPublicInfo) {
	return setToLocalStorage(STORE_KEY, accountsPublicInfo);
}

export async function getAccountPublicInfo(
	accountAddress: string,
): Promise<AccountPublicInfo | null> {
	return (await getStoredAccountsPublicInfo())[accountAddress] || null;
}

export type AccountsPublicInfoUpdates = {
	accountAddress: string;
	changes: Partial<AccountPublicInfo>;
}[];

export async function updateAccountsPublicInfo(accountsUpdates: AccountsPublicInfoUpdates) {
	const allAccountsPublicInfo = await getStoredAccountsPublicInfo();
	for (const { accountAddress, changes } of accountsUpdates) {
		allAccountsPublicInfo[accountAddress] = {
			...allAccountsPublicInfo[accountAddress],
			...changes,
		};
	}
	return storeAccountsPublicInfo(allAccountsPublicInfo);
}

export async function deleteAccountsPublicInfo(
	accounts: { toDelete: string[] } | { toKeep: string[] },
) {
	const allAccountsPublicInfo = await getStoredAccountsPublicInfo();
	const newAccountsPublicInfo: AccountsPublicInfo = {};
	for (const [anAddress, anAccountInfo] of Object.entries(allAccountsPublicInfo)) {
		if (
			('toDelete' in accounts && !accounts.toDelete.includes(anAddress)) ||
			('toKeep' in accounts && accounts.toKeep.includes(anAddress))
		) {
			newAccountsPublicInfo[anAddress] = anAccountInfo;
		}
	}
	return storeAccountsPublicInfo(newAccountsPublicInfo);
}
