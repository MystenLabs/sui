// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type LedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import { Ed25519PublicKey } from '@mysten/sui/keypairs/ed25519';
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useSuiLedgerClient } from './SuiLedgerClientProvider';

export type DerivedLedgerAccount = Pick<
	LedgerAccountSerializedUI,
	'address' | 'publicKey' | 'type' | 'derivationPath'
>;
type UseDeriveLedgerAccountOptions = {
	numAccountsToDerive: number;
} & Pick<UseQueryOptions<DerivedLedgerAccount[], unknown>, 'select'>;

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
		gcTime: 0,
	});
}

async function deriveAccountsFromLedger(
	suiLedgerClient: SuiLedgerClient,
	numAccountsToDerive: number,
) {
	const ledgerAccounts: DerivedLedgerAccount[] = [];
	const derivationPaths = getDerivationPathsForLedger(numAccountsToDerive);

	for (const derivationPath of derivationPaths) {
		const publicKeyResult = await suiLedgerClient.getPublicKey(derivationPath);
		const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
		const suiAddress = publicKey.toSuiAddress();
		ledgerAccounts.push({
			type: 'ledger',
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
