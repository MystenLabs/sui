// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair, type SerializedSignature } from '@mysten/sui.js/cryptography';
import { isBasePayload } from './BasePayload';
import { type AccountSourceSerializedUI } from '_src/background/account-sources/AccountSource';
import { type SerializedUIAccount } from '_src/background/accounts/Account';

import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { type Status } from '_src/background/storage-migration';
import type { Payload } from './Payload';

export type UIAccessibleEntityType = 'accountSources' | 'accounts';
export type LedgerAccountsPublicKeys = {
	accountID: string;
	publicKey: string;
}[];

type MethodPayloads = {
	getStoredEntities: { type: UIAccessibleEntityType };
	storedEntitiesResponse: { entities: any; type: UIAccessibleEntityType };
	createAccountSource: {
		type: 'mnemonic';
		params: {
			password: string;
			entropy?: string;
		};
	};
	accountSourceCreationResponse: { accountSource: AccountSourceSerializedUI };
	lockAccountSourceOrAccount: { id: string };
	unlockAccountSourceOrAccount: { id: string; password?: string };
	createAccounts:
		| { type: 'mnemonic-derived'; sourceID: string }
		| { type: 'imported'; keyPair: ExportedKeypair; password: string }
		| {
				type: 'ledger';
				accounts: { publicKey: string; derivationPath: string; address: string }[];
				password: string;
		  }
		| {
				type: 'zk';
				provider: ZkProvider;
		  };
	accountsCreatedResponse: { accounts: SerializedUIAccount[] };
	signData: { data: string; id: string };
	signDataResponse: { signature: SerializedSignature };
	entitiesUpdated: { type: UIAccessibleEntityType };
	getStorageMigrationStatus: null;
	storageMigrationStatus: { status: Status };
	doStorageMigration: { password: string };
	switchAccount: { accountID: string };
	setAccountNickname: { id: string; nickname: string | null };
	verifyPassword: { password: string };
	storeLedgerAccountsPublicKeys: { publicKeysToStore: LedgerAccountsPublicKeys };
	getAccountSourceEntropy: { accountSourceID: string };
	getAccountSourceEntropyResponse: { entropy: string };
	clearWallet: {};
};

type Methods = keyof MethodPayloads;

export interface MethodPayload<M extends Methods> {
	type: 'method-payload';
	method: M;
	args: MethodPayloads[M];
}

export function isMethodPayload<M extends Methods>(
	payload: Payload,
	method: M,
): payload is MethodPayload<M> {
	return (
		isBasePayload(payload) &&
		payload.type === 'method-payload' &&
		'method' in payload &&
		payload.method === method &&
		'args' in payload
	);
}
