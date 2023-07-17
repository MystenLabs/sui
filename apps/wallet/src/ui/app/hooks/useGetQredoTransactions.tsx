// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useActiveAddress } from './useActiveAddress';
import useAppSelector from './useAppSelector';
import { useQredoAPI } from './useQredoAPI';
import { API_ENV_TO_QREDO_NETWORK } from '../QredoSigner';
import { type TransactionStatus } from '_src/shared/qredo-api';

export function useGetQredoTransactions({
	qredoID,
	filterStatus,
	forceDisabled,
}: {
	qredoID?: string;
	filterStatus?: TransactionStatus[];
	forceDisabled?: boolean;
}) {
	const [qredoAPI] = useQredoAPI(qredoID);
	const apiEnv = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	const networkName = API_ENV_TO_QREDO_NETWORK[apiEnv] || null;
	const activeAddress = useActiveAddress();
	return useQuery({
		queryKey: [
			'get',
			'qredo',
			'transacions',
			qredoAPI,
			qredoID,
			networkName,
			activeAddress,
			filterStatus,
		],
		queryFn: () =>
			qredoAPI!.getTransactions({
				network: networkName!,
				address: activeAddress!,
			}),
		select: ({ list }) =>
			list.filter(({ status }) => !filterStatus?.length || filterStatus.includes(status)),
		enabled: !!(qredoAPI && qredoID && networkName && activeAddress && !forceDisabled),
		staleTime: 5000,
		refetchInterval: 5000,
	});
}
