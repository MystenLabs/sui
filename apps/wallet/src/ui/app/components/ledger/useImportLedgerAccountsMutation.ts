// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';
import { type Message } from '_src/shared/messaging/messages';

type UseImportLedgerAccountsMutationOptions = Pick<
	UseMutationOptions<Message, unknown, SerializedLedgerAccount[], unknown>,
	'onSuccess' | 'onError'
>;

export function useImportLedgerAccountsMutation({
	onSuccess,
	onError,
}: UseImportLedgerAccountsMutationOptions) {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationFn: (ledgerAccounts: SerializedLedgerAccount[]) => {
			return backgroundClient.importLedgerAccounts(ledgerAccounts);
		},
		onSuccess,
		onError,
	});
}
