// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB58 } from '@mysten/bcs';
import type {
	MoveValue,
	ProtocolConfigValue,
	SuiArgument,
	SuiClient,
	SuiMoveNormalizedModule,
} from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeStructTag, normalizeSuiAddress, parseStructTag } from '@mysten/sui/utils';

import type {
	ObjectFilter,
	QueryEventsQueryVariables,
	Rpc_Checkpoint_FieldsFragment,
	Rpc_Transaction_FieldsFragment,
} from './generated/queries.js';
import {
	DevInspectTransactionBlockDocument,
	DryRunTransactionBlockDocument,
	ExecuteTransactionBlockDocument,
	GetAllBalancesDocument,
	GetBalanceDocument,
	GetChainIdentifierDocument,
	GetCheckpointDocument,
	GetCheckpointsDocument,
	GetCoinMetadataDocument,
	GetCoinsDocument,
	GetCommitteeInfoDocument,
	GetCurrentEpochDocument,
	GetDynamicFieldObjectDocument,
	GetDynamicFieldsDocument,
	GetLatestCheckpointSequenceNumberDocument,
	GetLatestSuiSystemStateDocument,
	GetMoveFunctionArgTypesDocument,
	GetNormalizedMoveFunctionDocument,
	GetNormalizedMoveModuleDocument,
	GetNormalizedMoveModulesByPackageDocument,
	GetNormalizedMoveStructDocument,
	GetObjectDocument,
	GetOwnedObjectsDocument,
	GetProtocolConfigDocument,
	GetReferenceGasPriceDocument,
	GetStakesByIdsDocument,
	GetStakesDocument,
	GetTotalSupplyDocument,
	GetTotalTransactionBlocksDocument,
	GetTransactionBlockDocument,
	GetTypeLayoutDocument,
	GetValidatorsApyDocument,
	MultiGetObjectsDocument,
	MultiGetTransactionBlocksDocument,
	PaginateCheckpointTransactionBlocksDocument,
	PaginateEpochValidatorsDocument,
	PaginateMoveModuleListsDocument,
	PaginateTransactionBlockListsDocument,
	QueryEventsDocument,
	QueryTransactionBlocksDocument,
	ResolveNameServiceAddressDocument,
	ResolveNameServiceNamesDocument,
	TransactionBlockKindInput,
	TryGetPastObjectDocument,
} from './generated/queries.js';
import { mapJsonToBcs } from './mappers/bcs.js';
import { mapGraphQLCheckpointToRpcCheckpoint } from './mappers/checkpint.js';
import {
	mapNormalizedMoveFunction,
	mapNormalizedMoveModule,
	mapNormalizedMoveStruct,
	moveDataToRpcContent,
} from './mappers/move.js';
import { mapGraphQLMoveObjectToRpcObject, mapGraphQLObjectToRpcObject } from './mappers/object.js';
import { mapGraphQLStakeToRpcStake } from './mappers/stakes.js';
import { mapGraphQLTransactionBlockToRpcTransactionBlock } from './mappers/transaction-block.js';
import { isNumericString, toShortTypeString } from './mappers/util.js';
import { mapGraphQlValidatorToRpcValidator } from './mappers/validator.js';
import type { SuiClientGraphQLTransport } from './transport.js';

interface ResponseTypes {
	getRpcApiVersion: {
		info: { version?: string };
	};
}

export const RPC_METHODS: {
	[K in keyof SuiClient as SuiClient[K] extends (...args: any[]) => Promise<any>
		? K
		: never]?: SuiClient[K] extends (...args: any[]) => infer R
		? (
				transport: SuiClientGraphQLTransport,
				inputs: any[],
			) => K extends keyof ResponseTypes ? Promise<ResponseTypes[K]> : R
		: never;
} = {
	async getRpcApiVersion(transport) {
		const res = await transport.graphqlRequest({
			query: 'query { __typename }',
			variables: {},
		});

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		return {
			info: {
				version: res.headers.get('x-sui-rpc-version') ?? undefined,
			},
		};
	},

	async getCoins(transport, [owner, coinType, cursor, limit]) {
		const { nodes: coins, pageInfo } = await transport.graphqlQuery(
			{
				query: GetCoinsDocument,
				variables: {
					owner,
					type: coinType,
					cursor: cursor,
					first: limit,
				},
			},
			(data) => data.address?.coins,
		);

		return {
			data: coins.map((coin) => ({
				balance: coin.coinBalance,
				coinObjectId: coin.address,
				coinType: toShortTypeString(
					normalizeStructTag(parseStructTag(coin.contents?.type.repr!).typeParams[0]),
				),
				digest: coin.digest!,
				previousTransaction: coin.previousTransactionBlock?.digest!,
				version: String(coin.version!),
			})),
			nextCursor: pageInfo.endCursor,
			hasNextPage: pageInfo.hasNextPage,
		};
	},

	async getAllCoins(transport, inputs) {
		const { nodes: coins, pageInfo } = await transport.graphqlQuery(
			{
				query: GetCoinsDocument,
				variables: {
					owner: inputs[0],
					cursor: inputs[1],
					first: inputs[2],
				},
			},
			(data) => data.address?.coins,
		);

		return {
			data: coins.map((coin) => ({
				balance: coin.coinBalance,
				coinObjectId: coin.address,
				coinType: toShortTypeString(
					normalizeStructTag(parseStructTag(coin.contents?.type.repr!).typeParams[0]),
				),
				digest: coin.digest!,
				previousTransaction: coin.previousTransactionBlock?.digest!,
				version: String(coin.version!),
			})),
			nextCursor: pageInfo.endCursor,
			hasNextPage: pageInfo.hasNextPage,
		};
	},

	async getBalance(transport, inputs) {
		const balance = await transport.graphqlQuery(
			{
				query: GetBalanceDocument,
				variables: {
					owner: inputs[0],
					type: inputs[1],
				},
			},
			(data) => data.address?.balance,
		);

		return {
			coinType: toShortTypeString(balance.coinType?.repr!),
			coinObjectCount: balance.coinObjectCount!,
			totalBalance: balance.totalBalance,
			lockedBalance: {},
		};
	},

	async getAllBalances(transport, inputs) {
		const balances = await transport.graphqlQuery(
			{
				query: GetAllBalancesDocument,
				variables: {
					owner: inputs[0],
				},
			},
			(data) => data.address?.balances?.nodes,
		);

		return balances.map((balance) => ({
			coinType: toShortTypeString(balance.coinType?.repr!),
			coinObjectCount: balance.coinObjectCount!,
			totalBalance: balance.totalBalance,
			lockedBalance: {},
		}));
	},
	async getCoinMetadata(transport, inputs) {
		const metadata = await transport.graphqlQuery(
			{
				query: GetCoinMetadataDocument,
				variables: {
					coinType: inputs[0],
				},
			},
			(data) => data.coinMetadata,
		);

		return {
			decimals: metadata.decimals!,
			name: metadata.name!,
			symbol: metadata.symbol!,
			description: metadata.description!,
			iconUrl: metadata.iconUrl,
			id: metadata.address,
		};
	},
	async getTotalSupply(transport, inputs) {
		const metadata = await transport.graphqlQuery(
			{
				query: GetTotalSupplyDocument,
				variables: {
					coinType: inputs[0],
				},
			},
			(data) => data.coinMetadata,
		);

		return {
			value: (BigInt(metadata.supply!) * 10n ** BigInt(metadata.decimals!)).toString(),
		};
	},
	async getMoveFunctionArgTypes(transport, [pkg, module, fn]) {
		const moveModule = await transport.graphqlQuery(
			{
				query: GetMoveFunctionArgTypesDocument,
				variables: {
					module: module,
					packageId: pkg,
					function: fn,
				},
			},
			(data) => data.object?.asMovePackage?.module?.function?.parameters,
		);

		return moveModule.map((parameter) => {
			if (!parameter.signature.body.datatype) {
				return 'Pure';
			}

			return {
				Object:
					parameter.signature.ref === '&'
						? 'ByImmutableReference'
						: parameter.signature.ref === '&mut'
							? 'ByMutableReference'
							: 'ByValue',
			};
		});
	},
	async getNormalizedMoveFunction(transport, [pkg, module, fn]) {
		const moveFunction = await transport.graphqlQuery(
			{
				query: GetNormalizedMoveFunctionDocument,
				variables: {
					module: module,
					packageId: pkg,
					function: fn,
				},
			},
			(data) => data.object?.asMovePackage?.module?.function,
		);

		return mapNormalizedMoveFunction(moveFunction);
	},
	async getNormalizedMoveModulesByPackage(transport, [pkg]) {
		const movePackage = await transport.graphqlQuery(
			{
				query: GetNormalizedMoveModulesByPackageDocument,
				variables: {
					packageId: pkg,
				},
			},
			(data) => data.object?.asMovePackage,
		);

		let hasNextPage = movePackage.modules?.pageInfo.hasNextPage ?? false;
		let cursor = movePackage.modules?.pageInfo.endCursor;
		while (hasNextPage) {
			const page = await transport.graphqlQuery(
				{
					query: GetNormalizedMoveModulesByPackageDocument,
					variables: {
						packageId: pkg,
						cursor,
					},
				},
				(data) => data.object?.asMovePackage,
			);

			movePackage.modules?.nodes.push(...(page.modules?.nodes ?? []));
			hasNextPage = page.modules?.pageInfo.hasNextPage ?? false;
			cursor = page.modules?.pageInfo.endCursor;
		}

		const address = toShortTypeString(movePackage.address);
		const modules: Record<string, SuiMoveNormalizedModule> = {};

		for (const moveModule of movePackage.modules?.nodes ?? []) {
			let hasMoreFriends = moveModule.friends?.pageInfo.hasNextPage ?? false;
			let hasMoreFunctions = moveModule.functions?.pageInfo.hasNextPage ?? false;
			let hasMoreStructs = moveModule.structs?.pageInfo.hasNextPage ?? false;
			let afterFriends = moveModule.friends?.pageInfo.endCursor;
			let afterFunctions = moveModule.functions?.pageInfo.endCursor;
			let afterStructs = moveModule.structs?.pageInfo.endCursor;

			while (hasMoreFriends || hasMoreStructs || hasMoreFunctions) {
				const page = await transport.graphqlQuery(
					{
						query: PaginateMoveModuleListsDocument,
						variables: {
							module: moveModule.name,
							packageId: pkg,
							hasMoreFriends,
							hasMoreFunctions,
							hasMoreStructs,
							afterFriends,
							afterFunctions,
							afterStructs,
						},
					},
					(data) => data.object?.asMovePackage?.module,
				);

				moveModule.friends.nodes.push(...(page.friends?.nodes ?? []));
				moveModule.functions?.nodes.push(...(page.functions?.nodes ?? []));
				moveModule.structs?.nodes.push(...(page.structs?.nodes ?? []));
				hasMoreFriends = page.friends?.pageInfo.hasNextPage ?? false;
				hasMoreFunctions = page.functions?.pageInfo.hasNextPage ?? false;
				hasMoreStructs = page.structs?.pageInfo.hasNextPage ?? false;
				afterFriends = page.friends?.pageInfo.endCursor;
				afterFunctions = page.functions?.pageInfo.endCursor;
				afterStructs = page.structs?.pageInfo.endCursor;
			}
		}

		movePackage.modules?.nodes.forEach((module) => {
			modules[module.name] = mapNormalizedMoveModule(module, address);
		});

		return modules;
	},
	async getNormalizedMoveModule(transport, [pkg, module]) {
		const moveModule = await transport.graphqlQuery(
			{
				query: GetNormalizedMoveModuleDocument,
				variables: {
					module,
					packageId: pkg,
				},
			},
			(data) => data.object?.asMovePackage?.module,
		);

		let hasMoreFriends = moveModule.friends?.pageInfo.hasNextPage ?? false;
		let hasMoreFunctions = moveModule.functions?.pageInfo.hasNextPage ?? false;
		let hasMoreStructs = moveModule.structs?.pageInfo.hasNextPage ?? false;
		let afterFriends = moveModule.friends?.pageInfo.endCursor;
		let afterFunctions = moveModule.functions?.pageInfo.endCursor;
		let afterStructs = moveModule.structs?.pageInfo.endCursor;

		while (hasMoreFriends || hasMoreStructs || hasMoreFunctions) {
			const page = await transport.graphqlQuery(
				{
					query: PaginateMoveModuleListsDocument,
					variables: {
						module,
						packageId: pkg,
						hasMoreFriends,
						hasMoreFunctions,
						hasMoreStructs,
						afterFriends,
						afterFunctions,
						afterStructs,
					},
				},
				(data) => data.object?.asMovePackage?.module,
			);

			moveModule.friends.nodes.push(...(page.friends?.nodes ?? []));
			moveModule.functions?.nodes.push(...(page.functions?.nodes ?? []));
			moveModule.structs?.nodes.push(...(page.structs?.nodes ?? []));
			hasMoreFriends = page.friends?.pageInfo.hasNextPage ?? false;
			hasMoreFunctions = page.functions?.pageInfo.hasNextPage ?? false;
			hasMoreStructs = page.structs?.pageInfo.hasNextPage ?? false;
			afterFriends = page.friends?.pageInfo.endCursor;
			afterFunctions = page.functions?.pageInfo.endCursor;
			afterStructs = page.structs?.pageInfo.endCursor;
		}

		return mapNormalizedMoveModule(moveModule, normalizeSuiAddress(pkg));
	},
	async getNormalizedMoveStruct(transport, [pkg, module, struct]) {
		const moveStruct = await transport.graphqlQuery(
			{
				query: GetNormalizedMoveStructDocument,
				variables: {
					packageId: pkg,
					module,
					struct,
				},
			},
			(data) => data.object?.asMovePackage?.module?.struct,
		);

		return mapNormalizedMoveStruct(moveStruct);
	},
	async getOwnedObjects(transport, [owner, { filter: inputFilter, options }, cursor, limit]) {
		let filter: ObjectFilter | undefined;
		let typeFilter: string | undefined;

		if (inputFilter) {
			if ('Package' in inputFilter) {
				typeFilter = inputFilter.Package;
			} else if ('MoveModule' in inputFilter) {
				typeFilter = `${inputFilter.MoveModule.package}::${inputFilter.MoveModule.module}`;
			} else if ('StructType' in inputFilter) {
				typeFilter = inputFilter.StructType;
			}

			filter = {
				objectIds:
					'ObjectIds' in inputFilter
						? inputFilter.ObjectIds
						: 'ObjectId' in inputFilter
							? [inputFilter.ObjectId]
							: undefined,
				type: typeFilter,
				owner:
					'ObjectOwner' in inputFilter
						? inputFilter.ObjectOwner
						: 'AddressOwner' in inputFilter
							? inputFilter.AddressOwner
							: undefined,
			};
			const unsupportedFilters = ['MatchAll', 'MatchAny', 'MatchNone', 'Version'];

			for (const unsupportedFilter of unsupportedFilters) {
				if (unsupportedFilter in inputFilter) {
					throw new UnsupportedParamError('getOwnedObjects', unsupportedFilter);
				}
			}
		}

		const { nodes: objects, pageInfo } = await transport.graphqlQuery(
			{
				query: GetOwnedObjectsDocument,
				variables: {
					owner,
					limit,
					cursor,
					showBcs: options?.showBcs,
					showContent: options?.showContent,
					showOwner: options?.showOwner,
					showPreviousTransaction: options?.showPreviousTransaction,
					showStorageRebate: options?.showStorageRebate,
					showType: options?.showType,
					filter,
				},
			},
			(data) => data.address?.objects,
		);

		return {
			hasNextPage: pageInfo.hasNextPage,
			nextCursor: pageInfo.endCursor,
			data: objects.map((object) => ({
				data: mapGraphQLMoveObjectToRpcObject(object, options ?? {}),
			})),
		};
	},
	async getObject(transport, [id, options]) {
		const object = await transport.graphqlQuery(
			{
				query: GetObjectDocument,
				variables: {
					id,
					showBcs: options?.showBcs,
					showContent: options?.showContent,
					showOwner: options?.showOwner,
					showPreviousTransaction: options?.showPreviousTransaction,
					showStorageRebate: options?.showStorageRebate,
					showType: options?.showType,
				},
			},
			(data) => data.object,
		);

		return {
			data: mapGraphQLObjectToRpcObject(object, options ?? {}),
		};
	},
	async tryGetPastObject(transport, [id, version, options]) {
		const data = await transport.graphqlQuery({
			query: TryGetPastObjectDocument,
			variables: {
				id,
				version,
				showBcs: options?.showBcs,
				showContent: options?.showContent,
				showOwner: options?.showOwner,
				showPreviousTransaction: options?.showPreviousTransaction,
				showStorageRebate: options?.showStorageRebate,
				showType: options?.showType,
			},
		});

		if (!data.current) {
			return {
				details: 'Could not find the referenced object',
				status: 'ObjectNotExists',
			};
		}

		if (!data.object) {
			return data.current.version < Number(version)
				? {
						status: 'VersionTooHigh',
						details: {
							asked_version: String(version),
							latest_version: String(data.current.version),
							object_id: data.current.address,
						},
					}
				: {
						status: 'VersionNotFound',
						details: [data.current.address, String(version)],
					};
		}

		return {
			status: 'VersionFound',
			details: mapGraphQLObjectToRpcObject(data.object, options ?? {}),
		};
	},
	async multiGetObjects(transport, [ids, options]) {
		const objects = await transport.graphqlQuery(
			{
				query: MultiGetObjectsDocument,
				variables: {
					ids,
					showBcs: options?.showBcs,
					showContent: options?.showContent,
					showOwner: options?.showOwner,
					showPreviousTransaction: options?.showPreviousTransaction,
					showStorageRebate: options?.showStorageRebate,
					showType: options?.showType,
					limit: ids.length,
				},
			},
			(data) => data.objects?.nodes,
		);

		return objects.map((object) => ({
			data: mapGraphQLObjectToRpcObject(object, options ?? {}),
		}));
	},
	async queryTransactionBlocks(transport, [{ filter, options }, cursor, limit = 20, descending]) {
		const pagination = descending
			? {
					last: limit,
					before: cursor,
				}
			: {
					first: limit,
					after: cursor,
				};

		const unsupportedFilters = ['FromOrToAddress', 'FromAndToAddress', 'TransactionKindIn'];

		if (filter) {
			for (const unsupportedFilter of unsupportedFilters) {
				if (unsupportedFilter in filter) {
					throw new UnsupportedParamError('queryTransactionBlocks', unsupportedFilter);
				}
			}
		}

		const { nodes: transactionBlocks, pageInfo } = await transport.graphqlQuery(
			{
				query: QueryTransactionBlocksDocument,
				variables: {
					...pagination,
					showBalanceChanges: options?.showBalanceChanges,
					showEffects: options?.showEffects,
					showRawEffects: options?.showRawEffects,
					showObjectChanges: options?.showObjectChanges,
					showRawInput: options?.showRawInput,
					showInput: options?.showInput,
					filter: filter
						? {
								atCheckpoint:
									'Checkpoint' in filter ? Number.parseInt(filter.Checkpoint) : undefined,
								function:
									'MoveFunction' in filter
										? `${filter.MoveFunction.package}::${filter.MoveFunction.module}::${filter.MoveFunction.function}`
										: undefined,
								inputObject: 'InputObject' in filter ? filter.InputObject : undefined,
								changedObject: 'ChangedObject' in filter ? filter.ChangedObject : undefined,
								signAddress: 'FromAddress' in filter ? filter.FromAddress : undefined,
								recvAddress: 'ToAddress' in filter ? filter.ToAddress : undefined,
								kind:
									'TransactionKind' in filter
										? filter.TransactionKind === 'ProgrammableTransaction'
											? TransactionBlockKindInput.ProgrammableTx
											: TransactionBlockKindInput.SystemTx
										: undefined,
							}
						: {},
				},
			},
			(data) => data.transactionBlocks,
		);

		for (const transactionBlock of transactionBlocks) {
			await paginateTransactionBlockLists(transport, transactionBlock);
		}

		if (pagination.last) {
			transactionBlocks.reverse();
		}

		return {
			hasNextPage: pagination.last ? pageInfo.hasPreviousPage : pageInfo.hasNextPage,
			nextCursor: pagination.last ? pageInfo.startCursor : pageInfo.endCursor,
			data: transactionBlocks.map((transactionBlock) =>
				mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, options ?? {}),
			),
		};
	},
	async getTransactionBlock(transport, [digest, options]) {
		const transactionBlock = await transport.graphqlQuery(
			{
				query: GetTransactionBlockDocument,
				variables: {
					digest,
					showBalanceChanges: options?.showBalanceChanges,
					showEffects: options?.showEffects,
					showRawEffects: options?.showRawEffects,
					showObjectChanges: options?.showObjectChanges,
					showRawInput: options?.showRawInput,
					showInput: options?.showInput,
				},
			},
			(data) => data.transactionBlock,
		);

		await paginateTransactionBlockLists(transport, transactionBlock);

		return mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, options);
	},

	async multiGetTransactionBlocks(transport, [digests, options]) {
		const transactionBlocks = await transport.graphqlQuery(
			{
				query: MultiGetTransactionBlocksDocument,
				variables: {
					digests: digests,
					showBalanceChanges: options?.showBalanceChanges,
					showEffects: options?.showEffects,
					showRawEffects: options?.showEffects,
					showObjectChanges: options?.showObjectChanges,
					showRawInput: options?.showRawInput,
					showInput: options?.showInput,
					limit: digests.length,
				},
			},
			(data) => data.transactionBlocks?.nodes,
		);

		for (const transactionBlock of transactionBlocks) {
			await paginateTransactionBlockLists(transport, transactionBlock);
		}

		return transactionBlocks.map((transactionBlock) =>
			mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, options),
		);
	},
	async getTotalTransactionBlocks(transport): Promise<bigint> {
		return transport.graphqlQuery(
			{
				query: GetTotalTransactionBlocksDocument,
			},
			(data) => BigInt(data.checkpoint?.networkTotalTransactions!),
		);
	},
	async getReferenceGasPrice(transport) {
		const epoch = await transport.graphqlQuery(
			{
				query: GetReferenceGasPriceDocument,
				variables: {},
			},
			(data) => data.epoch,
		);

		return BigInt(epoch.referenceGasPrice);
	},
	async getStakes(transport, [owner]) {
		const stakes = await transport.graphqlQuery(
			{
				query: GetStakesDocument,
				variables: {
					owner,
				},
			},
			(data) => data.address?.stakedSuis?.nodes,
		);

		return mapGraphQLStakeToRpcStake(stakes);
	},
	async getStakesByIds(transport, [stakedSuiIds]) {
		const stakes = await transport.graphqlQuery(
			{
				query: GetStakesByIdsDocument,
				variables: {
					ids: stakedSuiIds,
				},
			},
			(data) => data.objects?.nodes.map((node) => node?.asMoveObject?.asStakedSui!).filter(Boolean),
		);

		return mapGraphQLStakeToRpcStake(stakes);
	},
	async getLatestSuiSystemState(transport) {
		const systemState = await transport.graphqlQuery(
			{
				query: GetLatestSuiSystemStateDocument,
			},
			(data) => data.epoch,
		);

		let hasMoreValidators =
			systemState.validatorSet?.activeValidators?.pageInfo.hasNextPage ?? false;
		let afterValidators = systemState.validatorSet?.activeValidators?.pageInfo.endCursor;

		while (hasMoreValidators) {
			const page = await transport.graphqlQuery(
				{
					query: PaginateEpochValidatorsDocument,
					variables: {
						id: systemState.epochId,
						after: afterValidators,
					},
				},
				(data) => data.epoch,
			);

			systemState.validatorSet?.activeValidators?.nodes.push(
				...(page.validatorSet?.activeValidators?.nodes ?? []),
			);
			hasMoreValidators = page.validatorSet?.activeValidators?.pageInfo.hasNextPage ?? false;
			afterValidators = page.validatorSet?.activeValidators?.pageInfo.endCursor;
		}

		return {
			activeValidators: systemState.validatorSet?.activeValidators?.nodes.map(
				mapGraphQlValidatorToRpcValidator,
			)!,
			atRiskValidators: systemState.validatorSet?.activeValidators.nodes
				?.filter((validator) => validator.atRisk)
				.map((validator) => [validator.address.address!, validator.atRisk!.toString()])!,
			epoch: String(systemState.epochId),
			epochDurationMs: String(
				new Date(systemState.endTimestamp).getTime() -
					new Date(systemState.startTimestamp).getTime(),
			),
			epochStartTimestampMs: String(new Date(systemState.startTimestamp).getTime()),
			inactivePoolsSize: String(systemState.validatorSet?.inactivePoolsSize),
			maxValidatorCount: String(systemState.systemParameters?.maxValidatorCount),
			minValidatorJoiningStake: String(systemState.systemParameters?.minValidatorJoiningStake),
			pendingActiveValidatorsSize: String(systemState.validatorSet?.pendingActiveValidatorsSize),
			pendingRemovals: systemState.validatorSet?.pendingRemovals?.map((idx) => String(idx)) ?? [],
			protocolVersion: String(systemState.protocolConfigs?.protocolVersion),
			referenceGasPrice: String(systemState.referenceGasPrice),
			safeMode: systemState.safeMode?.enabled!,
			safeModeComputationRewards: String(systemState.safeMode?.gasSummary?.computationCost),
			safeModeNonRefundableStorageFee: String(
				systemState.safeMode?.gasSummary?.nonRefundableStorageFee,
			),
			safeModeStorageRebates: String(systemState.safeMode?.gasSummary?.storageRebate),
			safeModeStorageRewards: String(systemState.safeMode?.gasSummary?.storageCost),
			stakeSubsidyBalance: String(systemState.systemStakeSubsidy?.balance),
			stakeSubsidyCurrentDistributionAmount: String(
				systemState.systemStakeSubsidy?.currentDistributionAmount,
			),
			stakeSubsidyDecreaseRate: systemState.systemStakeSubsidy?.decreaseRate!,
			stakeSubsidyDistributionCounter: String(systemState.systemStakeSubsidy?.distributionCounter),
			stakeSubsidyPeriodLength: String(systemState.systemStakeSubsidy?.periodLength),
			stakeSubsidyStartEpoch: String(systemState.systemParameters?.stakeSubsidyStartEpoch),
			stakingPoolMappingsSize: String(systemState.validatorSet?.stakingPoolMappingsSize),
			storageFundNonRefundableBalance: String(systemState.storageFund?.nonRefundableBalance),
			storageFundTotalObjectStorageRebates: String(
				systemState.storageFund?.totalObjectStorageRebates,
			),
			systemStateVersion: String(systemState.systemStateVersion),
			totalStake: systemState.validatorSet?.totalStake,
			validatorCandidatesSize: systemState.validatorSet?.validatorCandidatesSize?.toString()!,
			validatorLowStakeGracePeriod: systemState.systemParameters?.validatorLowStakeGracePeriod,
			validatorLowStakeThreshold: systemState.systemParameters?.validatorLowStakeThreshold,
			validatorReportRecords: [], // TODO
			validatorVeryLowStakeThreshold: systemState.systemParameters?.validatorVeryLowStakeThreshold,
			validatorCandidatesId: systemState.validatorSet?.validatorCandidatesId,
			inactivePoolsId: systemState.validatorSet?.inactivePoolsId,
			pendingActiveValidatorsId: systemState.validatorSet?.pendingActiveValidatorsId,
			stakingPoolMappingsId: systemState.validatorSet?.stakingPoolMappingsId,
		};
	},
	async queryEvents(transport, [query, cursor, limit, descending]) {
		const pagination: Partial<QueryEventsQueryVariables> = descending
			? { last: limit, before: cursor as never }
			: { first: limit, after: cursor as never };

		const filter: QueryEventsQueryVariables['filter'] = {
			sender: 'Sender' in query ? query.Sender : undefined,
			transactionDigest: 'Transaction' in query ? query.Transaction : undefined,
			eventType: 'MoveEventType' in query ? query.MoveEventType : undefined,
			emittingModule:
				'MoveModule' in query
					? `${query.MoveModule.package}::${query.MoveModule.module}`
					: undefined,
		};

		if ('MoveEventType' in query) {
			filter.eventType = query.MoveEventType;
		} else if ('MoveEventModule' in query) {
			filter.eventType = `${query.MoveEventModule.package}::${query.MoveEventModule.module}`;
		}

		const unsupportedFilters = [
			'Package',
			'MoveEventField',
			'Any',
			'All',
			'And',
			'Or',
			'TimeRange',
		];

		if (query) {
			for (const unsupportedFilter of unsupportedFilters) {
				if (unsupportedFilter in query) {
					throw new UnsupportedParamError('queryEvents', unsupportedFilter);
				}
			}
		}

		const { nodes: events, pageInfo } = await transport.graphqlQuery(
			{
				query: QueryEventsDocument,
				variables: {
					...pagination,
					filter,
				},
			},
			(data) => data.events,
		);

		if (pagination.last) {
			events.reverse();
		}

		return {
			hasNextPage: pagination.last ? pageInfo.hasPreviousPage : pageInfo.hasNextPage,
			nextCursor: (pagination.last ? pageInfo.startCursor : pageInfo.endCursor) as never,
			data: events.map((event) => ({
				bcs: event.bcs,
				id: {
					eventSeq: '', // TODO
					txDigest: '', // TODO
				},
				packageId: event.sendingModule?.package.address!,
				parsedJson: event.json ? JSON.parse(event.json) : undefined,
				sender: event.sender?.address,
				timestampMs: new Date(event.timestamp).getTime().toString(),
				transactionModule: `${event.sendingModule?.package.address}::${event.sendingModule?.name}`,
				type: toShortTypeString(event.type?.repr)!,
			})),
		};
	},
	async devInspectTransactionBlock(transport, [sender, devInspectTxBytes, gasPrice]) {
		const { transaction, error, results } = await transport.graphqlQuery(
			{
				query: DevInspectTransactionBlockDocument,
				variables: {
					txBytes: devInspectTxBytes,
					txMeta: {
						gasPrice: Number.parseInt(gasPrice),
						sender: sender,
					},
					showEffects: true,
					showEvents: true,
				},
			},
			(data) => data.dryRunTransactionBlock,
		);

		if (!transaction) {
			throw new Error('Unexpected error during dry run');
		}

		const result = mapGraphQLTransactionBlockToRpcTransactionBlock(transaction, {
			showEffects: true,
			showEvents: true,
		});

		return {
			error,
			effects: result.effects!,
			events: result.events!,
			results: results?.map((result) => ({
				mutableReferenceOutputs: result.mutatedReferences?.map(
					(ref): [SuiArgument, number[], string] => [
						ref.input.__typename === 'GasCoin'
							? 'GasCoin'
							: ref.input.__typename === 'Input'
								? {
										Input: ref.input.inputIndex,
									}
								: typeof ref.input.resultIndex === 'number'
									? {
											NestedResult: [ref.input.cmd, ref.input.resultIndex!] as [number, number],
										}
									: {
											Result: ref.input.cmd,
										},
						Array.from(fromB64(ref.bcs)),
						toShortTypeString(ref.type.repr),
					],
				),
				returnValues: result.returnValues?.map((value) => [
					Array.from(fromB64(value.bcs)),
					toShortTypeString(value.type.repr),
				]),
			})),
		};
	},
	async getDynamicFields(transport, [parentId, cursor, limit]) {
		const { nodes: fields, pageInfo } = await transport.graphqlQuery(
			{
				query: GetDynamicFieldsDocument,
				variables: {
					parentId,
					first: limit,
					cursor,
				},
			},
			(data) => data.owner?.dynamicFields,
		);

		return {
			data: fields.map((field) => ({
				bcsName: field.name?.bcs && toB58(fromB64(field.name.bcs)),
				digest: (field.value?.__typename === 'MoveObject' ? field.value.digest : undefined)!,
				name: {
					type: toShortTypeString(field.name?.type.repr)!,
					value: field.name?.json,
				},
				objectId: field.value?.__typename === 'MoveObject' ? field.value.address : undefined,
				objectType: (field.value?.__typename === 'MoveObject'
					? field.value.contents?.type.repr
					: field.value?.type.repr)!,
				type: field.value?.__typename === 'MoveObject' ? 'DynamicObject' : 'DynamicField',
				version: (field.value?.__typename === 'MoveObject'
					? field.value.version
					: undefined) as unknown as string,
			})),
			nextCursor: pageInfo.endCursor ?? null,
			hasNextPage: pageInfo.hasNextPage,
		};
	},
	async getDynamicFieldObject(transport, [parentId, name]) {
		const nameLayout = await transport.graphqlQuery(
			{
				query: GetTypeLayoutDocument,
				variables: {
					type: name.type,
				},
			},
			(data) => data.type.layout,
		);

		const bcsName = mapJsonToBcs(name.value, nameLayout);

		const parent = await transport.graphqlQuery(
			{
				query: GetDynamicFieldObjectDocument,
				variables: {
					parentId: parentId,
					name: {
						type: name.type,
						bcs: bcsName,
					},
				},
			},
			(data) => {
				return data.owner?.dynamicObjectField?.value?.__typename === 'MoveObject'
					? data.owner.dynamicObjectField.value.owner?.__typename === 'Parent'
						? data.owner.dynamicObjectField.value.owner.parent
						: undefined
					: undefined;
			},
		);

		return {
			data: {
				content: {
					dataType: 'moveObject' as const,
					...(moveDataToRpcContent(
						parent?.asMoveObject?.contents?.data!,
						parent?.asMoveObject?.contents?.type.layout!,
					) as {
						fields: {
							[key: string]: MoveValue;
						};
						type: string;
					}),
					hasPublicTransfer: parent?.asMoveObject?.hasPublicTransfer!,
				},
				digest: parent?.digest!,
				objectId: parent?.address,
				type: toShortTypeString(parent?.asMoveObject?.contents?.type.repr),
				version: parent?.version.toString()!,
				storageRebate: parent.storageRebate,
				previousTransaction: parent.previousTransactionBlock?.digest,
				owner:
					parent.owner?.__typename === 'Parent'
						? {
								ObjectOwner: parent.owner.parent?.address,
							}
						: undefined,
			},
		};
	},
	async executeTransactionBlock(transport, [txBytes, signatures, options, _requestType]) {
		// TODO: requestType
		const { effects, errors } = await transport.graphqlQuery(
			{
				query: ExecuteTransactionBlockDocument,
				variables: {
					txBytes,
					signatures,
					showBalanceChanges: options?.showBalanceChanges,
					showEffects: options?.showEffects,
					showRawEffects: options?.showRawEffects,
					showInput: options?.showInput,
					showEvents: options?.showEvents,
					showObjectChanges: options?.showObjectChanges,
					showRawInput: options?.showRawInput,
				},
			},
			(data) => data.executeTransactionBlock,
		);

		if (!effects?.transactionBlock) {
			const tx = Transaction.from(fromB64(txBytes));
			return { errors: errors ?? undefined, digest: await tx.getDigest() };
		}

		await paginateTransactionBlockLists(transport, effects.transactionBlock);

		return mapGraphQLTransactionBlockToRpcTransactionBlock(
			effects.transactionBlock,
			options,
			errors,
		);
	},
	async dryRunTransactionBlock(transport, [txBytes]) {
		const tx = Transaction.from(fromB64(txBytes));
		const { transaction, error } = await transport.graphqlQuery(
			{
				query: DryRunTransactionBlockDocument,
				variables: {
					txBytes,
					showBalanceChanges: true,
					showEffects: true,
					showEvents: true,
					showObjectChanges: true,
					showInput: true,
				},
			},
			(data) => data.dryRunTransactionBlock,
		);

		if (error || !transaction) {
			throw new Error(error ?? 'Unexpected error during dry run');
		}

		const result = mapGraphQLTransactionBlockToRpcTransactionBlock(
			{ ...transaction, digest: await tx.getDigest() },
			{
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showObjectChanges: true,
				showInput: true,
			},
		);

		return {
			input: result.transaction?.data!,
			balanceChanges: result.balanceChanges!,
			effects: result.effects!,
			events: result.events!,
			objectChanges: result.objectChanges!,
		};
	},
	async getLatestCheckpointSequenceNumber(transport) {
		const sequenceNumber = await transport.graphqlQuery(
			{
				query: GetLatestCheckpointSequenceNumberDocument,
			},
			(data) => data.checkpoint?.sequenceNumber,
		);

		return sequenceNumber.toString();
	},
	async getCheckpoint(transport, [id]) {
		const checkpoint = await transport.graphqlQuery(
			{
				query: GetCheckpointDocument,
				variables: {
					id:
						typeof id === 'number' || isNumericString(id)
							? {
									sequenceNumber: Number.parseInt(id.toString(), 10),
								}
							: {
									digest: id,
								},
				},
			},
			(data) => data.checkpoint,
		);

		await paginateCheckpointLists(transport, checkpoint);
		return mapGraphQLCheckpointToRpcCheckpoint(checkpoint);
	},
	async getCheckpoints(transport, [cursor, limit, descendingOrder]) {
		const pagination: Partial<QueryEventsQueryVariables> = descendingOrder
			? { last: limit, before: cursor as never }
			: { first: limit, after: cursor as never };

		const { nodes: checkpoints, pageInfo } = await transport.graphqlQuery(
			{
				query: GetCheckpointsDocument,
				variables: {
					...pagination,
				},
			},
			(data) => data.checkpoints,
		);

		for (const checkpoint of checkpoints) {
			await paginateCheckpointLists(transport, checkpoint);
		}

		if (pagination.last) {
			checkpoints.reverse();
		}

		return {
			hasNextPage: pagination.last ? pageInfo.hasPreviousPage : pageInfo.hasNextPage,
			nextCursor: (pagination.last ? pageInfo.startCursor : pageInfo.endCursor) as never,
			data: checkpoints.map((checkpoint) => mapGraphQLCheckpointToRpcCheckpoint(checkpoint)),
		};
	},
	async getCommitteeInfo(transport, [epoch]) {
		const { validatorSet, epochId } = await transport.graphqlQuery(
			{
				query: GetCommitteeInfoDocument,
				variables: {
					epochId: epoch ? Number.parseInt(epoch) : undefined,
				},
			},
			(data) => data.epoch,
		);

		let hasNextPage = validatorSet?.activeValidators?.pageInfo.hasNextPage;
		let after = validatorSet?.activeValidators?.pageInfo.endCursor;

		while (hasNextPage) {
			const page = await transport.graphqlQuery(
				{
					query: GetCommitteeInfoDocument,
					variables: {
						epochId: epoch ? Number.parseInt(epoch) : undefined,
						after,
					},
				},
				(data) => data.epoch?.validatorSet?.activeValidators,
			);

			validatorSet?.activeValidators.nodes.push(...page.nodes);
			hasNextPage = page.pageInfo.hasNextPage;
			after = page.pageInfo.endCursor;
		}

		return {
			epoch: epochId.toString(),
			validators: validatorSet?.activeValidators?.nodes.map((val) => [
				val.credentials?.protocolPubKey!,
				String(val.votingPower),
			])!,
		};
	},
	async getCurrentEpoch(transport) {
		const epoch = await transport.graphqlQuery(
			{
				query: GetCurrentEpochDocument,
			},
			(data) => data.epoch,
		);

		let hasNextPage = epoch.validatorSet?.activeValidators?.pageInfo.hasNextPage;
		let after = epoch.validatorSet?.activeValidators?.pageInfo.endCursor;

		while (hasNextPage) {
			const page = await transport.graphqlQuery(
				{
					query: PaginateEpochValidatorsDocument,
					variables: {
						id: epoch.epochId,
						after,
					},
				},
				(data) => data.epoch?.validatorSet?.activeValidators,
			);

			epoch.validatorSet?.activeValidators?.nodes.push(...page.nodes);
			hasNextPage = page.pageInfo.hasNextPage;
			after = page.pageInfo.endCursor;
		}

		return {
			epoch: String(epoch.epochId),
			validators: epoch.validatorSet?.activeValidators?.nodes.map(
				mapGraphQlValidatorToRpcValidator,
			)!,
			epochTotalTransactions: '0', // TODO
			firstCheckpointId: epoch.firstCheckpoint?.nodes[0]?.sequenceNumber.toString()!,
			endOfEpochInfo: null,
			referenceGasPrice: Number.parseInt(epoch.referenceGasPrice, 10),
			epochStartTimestamp: new Date(epoch.startTimestamp).getTime().toString(),
		};
	},
	async getValidatorsApy(transport) {
		const epoch = await transport.graphqlQuery(
			{
				query: GetValidatorsApyDocument,
			},
			(data) => data.epoch,
		);

		let hasNextPage = epoch.validatorSet?.activeValidators?.pageInfo.hasNextPage;
		let after = epoch.validatorSet?.activeValidators?.pageInfo.endCursor;

		while (hasNextPage) {
			const page = await transport.graphqlQuery(
				{
					query: PaginateEpochValidatorsDocument,
					variables: {
						id: epoch.epochId,
						after,
					},
				},
				(data) => data.epoch,
			);

			epoch.validatorSet?.activeValidators?.nodes.push(
				...(page.validatorSet?.activeValidators?.nodes ?? []),
			);
			hasNextPage = page.validatorSet?.activeValidators?.pageInfo.hasNextPage;
			after = page.validatorSet?.activeValidators?.pageInfo.endCursor;
		}

		return {
			epoch: String(epoch.epochId),
			apys: epoch.validatorSet?.activeValidators?.nodes.map((validator) => ({
				address: validator.address.address!,
				apy: (typeof validator.apy === 'number' ? validator.apy / 100 : null) as number,
			}))!,
		};
	},
	async getChainIdentifier(transport): Promise<string> {
		const identifier = await transport.graphqlQuery(
			{
				query: GetChainIdentifierDocument,
			},
			(data) => data.chainIdentifier,
		);

		return identifier;
	},
	async getProtocolConfig(transport, [version]) {
		const protocolConfig = await transport.graphqlQuery(
			{
				query: GetProtocolConfigDocument,
				variables: {
					protocolVersion: version ? Number.parseInt(version) : undefined,
				},
			},
			(data) => data.protocolConfig,
		);

		const featureFlags: Record<string, boolean> = {};
		const attributes: Record<string, ProtocolConfigValue | null> = {};

		const configTypeMap: Record<string, string> = {
			max_accumulated_txn_cost_per_object_in_narwhal_commit: 'u64',
			max_arguments: 'u32',
			max_gas_payment_objects: 'u32',
			max_modules_in_publish: 'u32',
			max_programmable_tx_commands: 'u32',
			max_pure_argument_size: 'u32',
			max_type_argument_depth: 'u32',
			max_type_arguments: 'u32',
			move_binary_format_version: 'u32',
			min_move_binary_format_version: 'u32',
			random_beacon_reduction_allowed_delta: 'u16',
			random_beacon_dkg_timeout_round: 'u32',
			random_beacon_reduction_lower_bound: 'u32',
			scoring_decision_cutoff_value: 'f64',
			scoring_decision_mad_divisor: 'f64',
			group_ops_bls12381_msm_max_len: 'u32',
			binary_module_handles: 'u16',
			binary_struct_handles: 'u16',
			binary_function_handles: 'u16',
			binary_function_instantiations: 'u16',
			binary_signatures: 'u16',
			binary_constant_pool: 'u16',
			binary_identifiers: 'u16',
			binary_address_identifiers: 'u16',
			binary_struct_defs: 'u16',
			binary_struct_def_instantiations: 'u16',
			binary_function_defs: 'u16',
			binary_field_handles: 'u16',
			binary_field_instantiations: 'u16',
			binary_friend_decls: 'u16',
			max_package_dependencies: 'u32',
			bridge_should_try_to_finalize_committee: 'bool',
		};

		for (const { key, value } of protocolConfig.configs) {
			attributes[key] =
				value === null
					? null
					: ({
							[configTypeMap[key] ?? 'u64']: value,
						} as ProtocolConfigValue);
		}

		for (const { key, value } of protocolConfig.featureFlags) {
			featureFlags[key] = value;
		}

		return {
			maxSupportedProtocolVersion: protocolConfig.protocolVersion?.toString(),
			minSupportedProtocolVersion: '1',
			protocolVersion: protocolConfig.protocolVersion?.toString(),
			attributes,
			featureFlags,
		};
	},
	async resolveNameServiceAddress(transport, [name]): Promise<string | null> {
		const data = await transport.graphqlQuery({
			query: ResolveNameServiceAddressDocument,
			variables: {
				domain: name,
			},
		});

		return data.resolveSuinsAddress?.address ?? null;
	},
	async resolveNameServiceNames(transport, [address, cursor, limit]) {
		const suinsRegistrations = await transport.graphqlQuery(
			{
				query: ResolveNameServiceNamesDocument,
				variables: {
					address: address,
					cursor,
					limit,
				},
			},
			(data) => data.address?.suinsRegistrations,
		);

		return {
			hasNextPage: suinsRegistrations.pageInfo.hasNextPage,
			nextCursor: suinsRegistrations.pageInfo.endCursor ?? null,
			data: suinsRegistrations?.nodes.map((node) => node.domain) ?? [],
		};
	},
};

export class UnsupportedParamError extends Error {
	constructor(method: string, param: string) {
		super(`Parameter ${param} is not supported for ${method} in the GraphQL API`);
	}
}

export class UnsupportedMethodError extends Error {
	constructor(method: string) {
		super(`Method ${method} is not supported in the GraphQL API`);
	}
}

async function paginateTransactionBlockLists(
	transport: SuiClientGraphQLTransport,
	transactionBlock: Rpc_Transaction_FieldsFragment,
) {
	let hasMoreEvents = transactionBlock.effects?.events?.pageInfo.hasNextPage ?? false;
	let hasMoreBalanceChanges =
		transactionBlock.effects?.balanceChanges?.pageInfo.hasNextPage ?? false;
	let hasMoreObjectChanges = transactionBlock.effects?.objectChanges?.pageInfo.hasNextPage ?? false;
	let afterEvents = transactionBlock.effects?.events?.pageInfo.endCursor;
	let afterBalanceChanges = transactionBlock.effects?.balanceChanges?.pageInfo.endCursor;
	let afterObjectChanges = transactionBlock.effects?.objectChanges?.pageInfo.endCursor;

	while (hasMoreEvents || hasMoreBalanceChanges || hasMoreObjectChanges) {
		const page = await transport.graphqlQuery(
			{
				query: PaginateTransactionBlockListsDocument,
				variables: {
					digest: transactionBlock.digest!,
					afterEvents,
					afterBalanceChanges,
					afterObjectChanges,
					hasMoreEvents,
					hasMoreBalanceChanges,
					hasMoreObjectChanges,
				},
			},
			(data) => data.transactionBlock?.effects,
		);

		transactionBlock.effects?.events?.nodes.push(...(page.events?.nodes ?? []));
		transactionBlock.effects?.balanceChanges?.nodes.push(...(page.balanceChanges?.nodes ?? []));
		transactionBlock.effects?.objectChanges?.nodes.push(...(page.objectChanges?.nodes ?? []));
		hasMoreEvents = page.events?.pageInfo.hasNextPage ?? false;
		hasMoreBalanceChanges = page.balanceChanges?.pageInfo.hasNextPage ?? false;
		hasMoreObjectChanges = page.objectChanges?.pageInfo.hasNextPage ?? false;
		afterEvents = page.events?.pageInfo.endCursor;
		afterBalanceChanges = page.balanceChanges?.pageInfo.endCursor;
		afterObjectChanges = page.objectChanges?.pageInfo.endCursor;
	}
}

async function paginateCheckpointLists(
	transport: SuiClientGraphQLTransport,
	checkpoint: Rpc_Checkpoint_FieldsFragment,
) {
	let hasNextPage = checkpoint.transactionBlocks.pageInfo.hasNextPage;
	let after = checkpoint.transactionBlocks.pageInfo.endCursor;
	while (hasNextPage) {
		const page = await transport.graphqlQuery(
			{
				query: PaginateCheckpointTransactionBlocksDocument,
				variables: {
					id: { digest: checkpoint.digest! },
					after,
				},
			},
			(data) => data.checkpoint?.transactionBlocks,
		);

		checkpoint.transactionBlocks.nodes.push(...page.nodes);
		hasNextPage = page.pageInfo.hasNextPage;
		after = page.pageInfo.endCursor;
	}

	const endOfEpochTx = checkpoint.endOfEpoch.nodes[0];

	if (
		endOfEpochTx?.kind?.__typename === 'EndOfEpochTransaction' &&
		endOfEpochTx.kind?.transactions.nodes[0].__typename === 'ChangeEpochTransaction' &&
		endOfEpochTx.kind.transactions.nodes[0].epoch?.epochId
	) {
		const validatorSet = endOfEpochTx.kind.transactions.nodes[0].epoch.validatorSet;
		let hasNextPage = validatorSet?.activeValidators.pageInfo.hasNextPage;
		let after = validatorSet?.activeValidators.pageInfo.endCursor;

		while (hasNextPage) {
			const page = await transport.graphqlQuery(
				{
					query: GetCommitteeInfoDocument,
					variables: {
						epochId: endOfEpochTx.kind.transactions.nodes[0].epoch?.epochId,
						after,
					},
				},
				(data) => data.epoch?.validatorSet?.activeValidators,
			);

			validatorSet?.activeValidators.nodes.push(...page.nodes);
			hasNextPage = page.pageInfo?.hasNextPage;
			after = page.pageInfo?.endCursor;
		}
	}
}
