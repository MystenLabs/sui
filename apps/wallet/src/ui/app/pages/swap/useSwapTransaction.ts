// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useSuiClient } from '@mysten/dapp-kit';
import { type DryRunTransactionBlockResponse } from '@mysten/sui/client';
import { useQuery, useQueryClient } from '@tanstack/react-query';

export type SwapRequest = {
	amount: string;
	fromType?: string;
	slippage: number;
	source: string;
	sender?: string;
	toType?: string;
};

export type SwapResponse = {
	bytes: string;
	error: string;
	fee: {
		percentage: number;
		address: string;
	};
	outAmount: string;
	provider: string;
};

export type SwapResult =
	| (SwapResponse & {
			dryRunResponse: DryRunTransactionBlockResponse;
	  })
	| null;

const getQueryKey = (params: SwapRequest) => ['swap', params];

async function* streamAsyncIterator<T>(stream: ReadableStream): AsyncGenerator<T> {
	const reader = stream.getReader();
	const decoder = new TextDecoder('utf-8');
	let buffer = '';

	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			buffer += decoder.decode(value, { stream: true });
			const lines = buffer.split('\n');
			buffer = lines.pop() || '';

			for (const line of lines) {
				if (line.trim()) {
					yield JSON.parse(line.trim());
				}
			}
		}
	} finally {
		reader.releaseLock();
	}
}

export function useSwapTransaction({ enabled, ...params }: SwapRequest & { enabled: boolean }) {
	const client = useSuiClient();
	const queryClient = useQueryClient();

	return useQuery({
		queryKey: getQueryKey(params),
		queryFn: async ({ signal }) => {
			const response = await fetch('https://apps-backend.sui.io/swap', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json',
				},
				body: JSON.stringify(params),
				signal,
			});

			if (!response.body || !response.ok) {
				throw new Error(`Failed to fetch swap data ${response.statusText}`);
			}

			for await (const swapResponse of streamAsyncIterator<SwapResponse>(response.body)) {
				if (!swapResponse) continue;
				if (swapResponse.error) throw new Error(swapResponse.error);

				const dryRunResponse = await client.dryRunTransactionBlock({
					transactionBlock: swapResponse.bytes,
				});

				queryClient.setQueryData<SwapResult>(getQueryKey(params), {
					dryRunResponse,
					...swapResponse,
				});
			}

			return queryClient.getQueryData<SwapResult>(getQueryKey(params)) ?? null;
		},
		staleTime: 0,
		enabled: enabled && !!params.amount && !!params.sender && !!params.fromType && !!params.toType,
	});
}
