// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { type DerivedLedgerAccount } from './useDeriveLedgerAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { type Message } from '_src/shared/messaging/messages';

type UseImportLedgerAccountsMutationOptions = Pick<
	UseMutationOptions<Message | SerializedUIAccount[], unknown, DerivedLedgerAccount[], unknown>,
	'onSuccess' | 'onError'
> & { password: string };

export function useImportLedgerAccountsMutation({
	onSuccess,
	onError,
	password,
}: UseImportLedgerAccountsMutationOptions) {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationFn: async (ledgerAccounts: DerivedLedgerAccount[]) =>
			backgroundClient.createAccounts({
				type: 'ledger',
				accounts: ledgerAccounts.map(({ address, derivationPath, publicKey }) => ({
					address,
					derivationPath,
					publicKey: publicKey!,
				})),
				password,
			}),
		onSuccess,
		onError,
	});
}
