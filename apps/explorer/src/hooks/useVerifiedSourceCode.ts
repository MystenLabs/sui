// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientContext } from '@mysten/dapp-kit';
import { useQuery } from '@tanstack/react-query';
import { Network } from '~/utils/api/DefaultRpcClient';

type UseVerifiedSourceCodeArgs = {
	packageId: string;
	moduleName: string;
};

type UseVerifiedSourceCodeResponse = {
	source?: string;
	error?: string;
};

const networksWithSourceCodeVerification: Network[] = [Network.TESTNET, Network.MAINNET];

/**
 * Hook that retrieves the source code for verified modules.
 */
export function useVerifiedSourceCode({ packageId, moduleName }: UseVerifiedSourceCodeArgs) {
	const { network } = useSuiClientContext();

	return useQuery({
		queryKey: ['verified-source-code', packageId, moduleName, network],
		queryFn: async () => {
			const response = await fetch(
				`http://source.mystenlabs.com/api?network=${network.toLowerCase()}&address=${packageId}&module=${moduleName}`,
			);
			if (!response.ok) {
				throw new Error(`Encountered un-expected response: ${response.status}`);
			}

			const jsonResponse: UseVerifiedSourceCodeResponse = await response.json();
			return jsonResponse.source || null;
		},
		enabled: networksWithSourceCodeVerification.includes(network as Network),
	});
}
