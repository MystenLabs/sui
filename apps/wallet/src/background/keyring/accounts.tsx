// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFromLocalStorage, setToLocalStorage } from '../storage-utils';

/**
 * @deprecated
 */
export type AccountPublicInfo = {
	// base64 of the public key - always ed25519 key
	publicKey: string | null;
};

const STORE_KEY = 'accountsPublicInfo';

type AccountsPublicInfo = Record<string, AccountPublicInfo>;

/**
 * @deprecated
 */
export async function getStoredAccountsPublicInfo(): Promise<AccountsPublicInfo> {
	return (await getFromLocalStorage(STORE_KEY, {})) || {};
}

/**
 * @deprecated
 */
export function storeAccountsPublicInfo(accountsPublicInfo: AccountsPublicInfo) {
	return setToLocalStorage(STORE_KEY, accountsPublicInfo);
}

/**
 * @deprecated
 */
export async function getAccountPublicInfo(
	accountAddress: string,
): Promise<AccountPublicInfo | null> {
	return (await getStoredAccountsPublicInfo())[accountAddress] || null;
}

/**
 * @deprecated
 */
export type AccountsPublicInfoUpdates = {
	accountAddress: string;
	changes: Partial<AccountPublicInfo>;
}[];

/**
 * @deprecated
 */
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

/**
 * @deprecated
 */
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
