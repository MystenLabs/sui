// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import { type Transaction } from '@mysten/sui/transactions';
import { useEffect } from 'react';

import { type RequestType } from './types';
import { useDappPreflight } from './useDappPreflight';

export function useShowScamWarning({
	url,
	requestType,
	transaction,
	message,
}: {
	url?: URL;
	requestType: RequestType;
	transaction?: Transaction;
	message?: string;
}) {
	const { data, isPending, isError } = useDappPreflight({
		requestType,
		origin: url?.origin,
		transaction,
		message,
	});

	useEffect(() => {
		if (data?.block.enabled && url?.hostname) {
			ampli.interactedWithMaliciousDomain({ hostname: url.hostname });
		}
	}, [data, url]);

	return {
		data,
		isOpen: !!data?.block.enabled,
		isPending,
		isError,
	};
}
