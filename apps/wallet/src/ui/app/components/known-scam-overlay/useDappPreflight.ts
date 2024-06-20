// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { type Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';

import { RequestType, type DappPreflightRequest, type DappPreflightResponse } from './types';

export function useDappPreflight({
	requestType,
	origin,
	transaction,
	message,
}: {
	requestType: RequestType;
	origin?: string;
	transaction?: Transaction;
	message?: string;
}) {
	const { request } = useAppsBackend();
	const client = useSuiClient();
	const transactionData = transaction?.getData();

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['dapp-preflight', { requestType, origin, transaction: transactionData, message }],
		queryFn: async () => {
			if (!origin) {
				throw new Error('No origin provided');
			}

			const body: DappPreflightRequest = {
				requestType,
				origin,
			};

			if (requestType === RequestType.SIGN_MESSAGE) {
				body.message = message;
			} else if (requestType === RequestType.SIGN_TRANSACTION && transaction) {
				const transactionBytes = await transaction.build({ client });
				body.transactionBytes = toB64(transactionBytes);
			}

			return request<DappPreflightResponse>(
				'v1/dapp-preflight',
				{},
				{
					method: 'POST',
					body: JSON.stringify(body),
					headers: { 'Content-Type': 'application/json' },
				},
			);
		},
		enabled: !!origin,
		staleTime: 5 * 60 * 1000,
	});
}
