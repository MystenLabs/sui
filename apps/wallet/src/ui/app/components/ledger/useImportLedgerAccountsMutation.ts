// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { type LedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';
import { type Message } from '_src/shared/messaging/messages';

type UseImportLedgerAccountsMutationOptions = Pick<
	UseMutationOptions<
		Message | SerializedUIAccount[],
		unknown,
		SerializedLedgerAccount[] | LedgerAccountSerializedUI[],
		unknown
	>,
	'onSuccess' | 'onError'
> & { password?: string };

export function useImportLedgerAccountsMutation({
	onSuccess,
	onError,
	password,
}: UseImportLedgerAccountsMutationOptions) {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationFn: async (ledgerAccounts: SerializedLedgerAccount[]) => {
			if (password) {
				return backgroundClient.createAccounts({
					type: 'ledger',
					accounts: ledgerAccounts.map(({ address, derivationPath, publicKey }) => ({
						address,
						derivationPath,
						publicKey: publicKey!,
					})),
					password,
				});
			}
			return backgroundClient.importLedgerAccounts(ledgerAccounts as SerializedLedgerAccount[]);
		},
		onSuccess,
		onError,
	});
}
