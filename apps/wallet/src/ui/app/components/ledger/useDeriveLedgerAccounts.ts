// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519PublicKey } from '@mysten/sui.js/keypairs/ed25519';
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useSuiLedgerClient } from './SuiLedgerClientProvider';
import { AccountType } from '_src/background/keyring/Account';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';

import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

type UseDeriveLedgerAccountOptions = {
	numAccountsToDerive: number;
} & Pick<UseQueryOptions<SerializedLedgerAccount[], unknown>, 'select' | 'onSuccess' | 'onError'>;

export function useDeriveLedgerAccounts(options: UseDeriveLedgerAccountOptions) {
	const { numAccountsToDerive, ...useQueryOptions } = options;
	const { suiLedgerClient } = useSuiLedgerClient();

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['derive-ledger-accounts'],
		queryFn: () => {
			if (!suiLedgerClient) {
				throw new Error("The Sui application isn't open on a connected Ledger device");
			}
			return deriveAccountsFromLedger(suiLedgerClient, numAccountsToDerive);
		},
		...useQueryOptions,
		cacheTime: 0,
	});
}

async function deriveAccountsFromLedger(
	suiLedgerClient: SuiLedgerClient,
	numAccountsToDerive: number,
) {
	const ledgerAccounts: SerializedLedgerAccount[] = [];
	const derivationPaths = getDerivationPathsForLedger(numAccountsToDerive);

	for (const derivationPath of derivationPaths) {
		const publicKeyResult = await suiLedgerClient.getPublicKey(derivationPath);
		const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
		const suiAddress = publicKey.toSuiAddress();
		ledgerAccounts.push({
			type: AccountType.LEDGER,
			address: suiAddress,
			derivationPath,
			publicKey: publicKey.toBase64(),
		});
	}

	return ledgerAccounts;
}

function getDerivationPathsForLedger(numDerivations: number) {
	return Array.from({
		length: numDerivations,
	}).map((_, index) => `m/44'/784'/${index}'/0'/0'`);
}
