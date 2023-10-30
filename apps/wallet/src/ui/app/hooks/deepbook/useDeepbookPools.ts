// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { DEEPBOOK_KEY } from '_pages/swap/constants';
import { useDeepBookContext } from '_shared/deepBook/context';
import { useQuery } from '@tanstack/react-query';

export function useDeepbookPools() {
	const deepBookClient = useDeepBookContext().client;

	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: () => deepBookClient.getAllPools({}),
	});
}
