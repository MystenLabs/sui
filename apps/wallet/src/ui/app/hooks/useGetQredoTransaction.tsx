// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useQredoAPI } from './useQredoAPI';

export function useGetQredoTransaction({
	qredoID,
	qredoTransactionID,
	forceDisabled,
}: {
	qredoID?: string;
	qredoTransactionID?: string;
	forceDisabled?: boolean;
}) {
	const [qredoAPI] = useQredoAPI(qredoID);
	return useQuery({
		queryKey: ['get', 'qredo', 'transacion', qredoAPI, qredoID, qredoTransactionID],
		queryFn: () => qredoAPI!.getTransaction(qredoTransactionID!),
		enabled: !!(qredoAPI && qredoID && qredoTransactionID && !forceDisabled),
		staleTime: 5000,
		refetchInterval: 5000,
	});
}
