// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient, useGetSystemState, isSuiNSName, useSuiNSEnabled } from '@mysten/core';
import {
	isValidTransactionDigest,
	isValidSuiAddress,
	isValidSuiObjectId,
	normalizeSuiObjectId,
	is,
	SuiObjectData,
	type JsonRpcProvider,
	getTransactionDigest,
	type SuiSystemStateSummary,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const isGenesisLibAddress = (value: string): boolean => /^(0x|0X)0{0,39}[12]$/.test(value);

type Result = {
	label: string;
	results: { id: string; label: string; type: string }[];
};

const getResultsForTransaction = async (rpc: JsonRpcProvider, query: string) => {
	if (!isValidTransactionDigest(query)) return null;
	const txdata = await rpc.getTransactionBlock({ digest: query });
	return {
		label: 'transaction',
		results: [
			{
				id: getTransactionDigest(txdata),
				label: getTransactionDigest(txdata),
				type: 'transaction',
			},
		],
	};
};

const getResultsForObject = async (rpc: JsonRpcProvider, query: string) => {
	const normalized = normalizeSuiObjectId(query);
	if (!isValidSuiObjectId(normalized)) return null;

	const { data, error } = await rpc.getObject({ id: normalized });
	if (!is(data, SuiObjectData) || error) return null;

	return {
		label: 'object',
		results: [
			{
				id: data.objectId,
				label: data.objectId,
				type: 'object',
			},
		],
	};
};

const getResultsForCheckpoint = async (rpc: JsonRpcProvider, query: string) => {
	// Checkpoint digests have the same format as transaction digests:
	if (!isValidTransactionDigest(query)) return null;

	const { digest } = await rpc.getCheckpoint({ id: query });
	if (!digest) return null;

	return {
		label: 'checkpoint',
		results: [
			{
				id: digest,
				label: digest,
				type: 'checkpoint',
			},
		],
	};
};

const getResultsForAddress = async (rpc: JsonRpcProvider, query: string, suiNSEnabled: boolean) => {
	if (suiNSEnabled && isSuiNSName(query)) {
		const resolved = await rpc.resolveNameServiceAddress({ name: query });
		if (!resolved) return null;
		return {
			label: 'address',
			results: [
				{
					id: resolved,
					label: resolved,
					type: 'address',
				},
			],
		};
	}

	const normalized = normalizeSuiObjectId(query);
	if (!isValidSuiAddress(normalized) || isGenesisLibAddress(normalized)) return null;

	const [from, to] = await Promise.all([
		rpc.queryTransactionBlocks({
			filter: { FromAddress: normalized },
			limit: 1,
		}),
		rpc.queryTransactionBlocks({
			filter: { ToAddress: normalized },
			limit: 1,
		}),
	]);

	if (!from.data?.length && !to.data?.length) return null;

	return {
		label: 'address',
		results: [
			{
				id: normalized,
				label: normalized,
				type: 'address',
			},
		],
	};
};

// Query for validator by pool id or sui address.
const getResultsForValidatorByPoolIdOrSuiAddress = async (
	systemStateSummery: SuiSystemStateSummary | null,
	query: string,
) => {
	const normalized = normalizeSuiObjectId(query);
	if ((!isValidSuiAddress(normalized) && !isValidSuiObjectId(normalized)) || !systemStateSummery)
		return null;

	// find validator by pool id or sui address
	const validator = systemStateSummery.activeValidators?.find(
		({ stakingPoolId, suiAddress }) => stakingPoolId === normalized || suiAddress === query,
	);

	if (!validator) return null;

	return {
		label: 'validator',
		results: [
			{
				id: validator.suiAddress || validator.stakingPoolId,
				label: normalized,
				type: 'validator',
			},
		],
	};
};

export function useSearch(query: string) {
	const rpc = useRpcClient();
	const { data: systemStateSummery } = useGetSystemState();
	const suiNSEnabled = useSuiNSEnabled();

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['search', query],
		queryFn: async () => {
			const results = (
				await Promise.allSettled([
					getResultsForTransaction(rpc, query),
					getResultsForCheckpoint(rpc, query),
					getResultsForAddress(rpc, query, suiNSEnabled),
					getResultsForObject(rpc, query),
					getResultsForValidatorByPoolIdOrSuiAddress(systemStateSummery || null, query),
				])
			).filter((r) => r.status === 'fulfilled' && r.value) as PromiseFulfilledResult<Result>[];

			return results.map(({ value }) => value);
		},
		enabled: !!query,
		cacheTime: 10000,
	});
}
