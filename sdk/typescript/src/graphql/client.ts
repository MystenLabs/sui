// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TypedDocumentNode } from '@graphql-typed-document-node/core';
import { print } from 'graphql';
import type { DocumentNode } from 'graphql';

import type { PaginationArguments, SuiClientOptions } from '../client/client.js';
import { SuiClient } from '../client/client.js';
import type {
	AddressMetrics,
	AllEpochsAddressMetrics,
	CheckpointPage,
	DynamicFieldPage,
	EpochInfo,
	EpochPage,
	MoveCallMetrics,
	NetworkMetrics,
	ResolvedNameServiceNames,
	SuiMoveNormalizedModules,
} from '../client/types/chain.js';
import type { CoinBalance } from '../client/types/coins.js';
import type { Unsubscribe } from '../client/types/common.js';
import type {
	Checkpoint,
	CoinMetadata,
	CoinSupply,
	CommitteeInfo,
	DelegatedStake,
	DevInspectResults,
	DryRunTransactionBlockResponse,
	MoveStruct,
	ObjectRead,
	PaginatedCoins,
	PaginatedEvents,
	PaginatedObjectsResponse,
	PaginatedTransactionResponse,
	ProtocolConfig,
	ProtocolConfigValue,
	SuiEvent,
	SuiMoveFunctionArgType,
	SuiMoveNormalizedFunction,
	SuiMoveNormalizedModule,
	SuiMoveNormalizedStruct,
	SuiObjectResponse,
	SuiSystemStateSummary,
	SuiTransactionBlockResponse,
	TransactionEffects,
	ValidatorsApy,
} from '../client/types/generated.js';
import type {
	DevInspectTransactionBlockParams,
	DryRunTransactionBlockParams,
	ExecuteTransactionBlockParams,
	GetAllBalancesParams,
	GetAllCoinsParams,
	GetBalanceParams,
	GetCheckpointParams,
	GetCheckpointsParams,
	GetCoinMetadataParams,
	GetCoinsParams,
	GetCommitteeInfoParams,
	GetDynamicFieldObjectParams,
	GetDynamicFieldsParams,
	GetMoveFunctionArgTypesParams,
	GetNormalizedMoveFunctionParams,
	GetNormalizedMoveModuleParams,
	GetNormalizedMoveModulesByPackageParams,
	GetNormalizedMoveStructParams,
	GetObjectParams,
	GetOwnedObjectsParams,
	GetProtocolConfigParams,
	GetStakesByIdsParams,
	GetStakesParams,
	GetTotalSupplyParams,
	GetTransactionBlockParams,
	MultiGetObjectsParams,
	MultiGetTransactionBlocksParams,
	QueryEventsParams,
	QueryTransactionBlocksParams,
	ResolveNameServiceAddressParams,
	ResolveNameServiceNamesParams,
	SubscribeEventParams,
	SubscribeTransactionParams,
	TryGetPastObjectParams,
} from '../client/types/params.js';
import { normalizeStructTag, parseStructTag } from '../utils/sui-types.js';
import type {
	ObjectFilter,
	QueryEventsQueryVariables,
	TransactionBlockKindInput,
} from './generated.js';
import {
	GetAllBalancesDocument,
	GetBalanceDocument,
	GetChainIdentifierDocument,
	GetCheckpointDocument,
	GetCheckpointsDocument,
	GetCoinMetadataDocument,
	GetCoinsDocument,
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
	GetValidatorsApyDocument,
	MultiGetObjectsDocument,
	MultiGetTransactionBlocksDocument,
	QueryEventsDocument,
	QueryTransactionBlocksDocument,
	ResolveNameServiceAddressDocument,
	ResolveNameServiceNamesDocument,
	TryGetPastObjectDocument,
	TypedDocumentString,
} from './generated.js';
import { formatDisplay } from './mappers/display.js';
import {
	mapNormalizedMoveFunction,
	mapNormalizedMoveModule,
	mapNormalizedMoveStruct,
	moveDataToRpcContent,
} from './mappers/move.js';
import { mapGraphQLMoveObjectToRpcObject, mapGraphQLObjectToRpcObject } from './mappers/object.js';
import { mapGraphQLTransactionBlockToRpcTransactionBlock } from './mappers/transaction-block.js';
import { isNumericString, toShortTypeString } from './mappers/util.js';
import { mapGraphQlValidatorToRpcValidator } from './mappers/validator.js';

export type GraphQLDocument<
	Result = Record<string, unknown>,
	Variables = Record<string, unknown>,
> =
	| string
	| DocumentNode
	| TypedDocumentNode<Result, Variables>
	| TypedDocumentString<Result, Variables>;

export type GraphQLQueryOptions<
	Result = Record<string, unknown>,
	Variables = Record<string, unknown>,
> = {
	query: GraphQLDocument<Result, Variables>;
	operationName?: string;
	extensions?: Record<string, unknown>;
} & (Variables extends { [key: string]: never }
	? { variables?: Variables }
	: {
			variables: Variables;
	  });

export type GraphQLQueryResult<Result = Record<string, unknown>> = {
	data?: Result;
	errors?: GraphQLResponseErrors;
	extensions?: Record<string, unknown>;
};

export type GraphQLResponseErrors = Array<{
	message: string;
	locations?: { line: number; column: number }[];
	path?: (string | number)[];
}>;

export class GraphQLSuiClient extends SuiClient {
	#graphqlURL: string;

	constructor({ graphqlURL, ...options }: SuiClientOptions & { graphqlURL: string }) {
		super(options);
		this.#graphqlURL = graphqlURL;
	}

	async graphqlQuery<Result = Record<string, unknown>, Variables = Record<string, unknown>>(
		options: GraphQLQueryOptions<Result, Variables>,
	): Promise<GraphQLQueryResult<Result>> {
		const res = await fetch(this.#graphqlURL, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({
				query:
					typeof options.query === 'string' || options.query instanceof TypedDocumentString
						? options.query.toString()
						: print(options.query),
				variables: options.variables,
				extensions: options.extensions,
				operationName: options.operationName,
			}),
		});

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		return res.json();
	}

	async #graphqlQuery<
		Result = Record<string, unknown>,
		Variables = Record<string, unknown>,
		Data = Result,
	>(
		options: GraphQLQueryOptions<Result, Variables>,
		getData?: (result: Result) => Data,
	): Promise<NonNullable<Data>> {
		const { data, errors } = await this.graphqlQuery(options);

		handleGraphQLErrors(errors);

		const extractedData = data && (getData ? getData(data) : data);

		if (extractedData == null) {
			throw new Error('Missing response data');
		}

		return extractedData as NonNullable<Data>;
	}

	override async getRpcApiVersion(): Promise<string | undefined> {
		const res = await fetch(this.#graphqlURL, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({
				query: 'query { __typename }',
			}),
		});

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		return res.headers.get('x-sui-rpc-version') ?? undefined;
	}

	override async getCoins(input: GetCoinsParams): Promise<PaginatedCoins> {
		const { nodes: coins, pageInfo } = await this.#graphqlQuery(
			{
				query: GetCoinsDocument,
				variables: {
					owner: input.owner,
					type: input.coinType,
					first: input.limit,
					cursor: input.cursor,
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
	}

	override getAllCoins(input: GetAllCoinsParams): Promise<PaginatedCoins> {
		return this.getCoins({
			...input,
			coinType: null,
		});
	}
	override async getBalance(input: GetBalanceParams): Promise<CoinBalance> {
		const balance = await this.#graphqlQuery(
			{
				query: GetBalanceDocument,
				variables: {
					owner: input.owner,
					type: input.coinType,
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
	}
	override async getAllBalances(input: GetAllBalancesParams): Promise<CoinBalance[]> {
		const balances = await this.#graphqlQuery(
			{
				query: GetAllBalancesDocument,
				variables: {
					owner: input.owner,
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
	}

	override async getCoinMetadata(input: GetCoinMetadataParams): Promise<CoinMetadata | null> {
		const metadata = await this.#graphqlQuery(
			{
				query: GetCoinMetadataDocument,
				variables: {
					coinType: input.coinType,
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
	}

	override async getTotalSupply(input: GetTotalSupplyParams): Promise<CoinSupply> {
		const metadata = await this.#graphqlQuery(
			{
				query: GetTotalSupplyDocument,
				variables: {
					coinType: input.coinType,
				},
			},
			(data) => data.coinMetadata,
		);

		return {
			value: (BigInt(metadata.supply!) * 10n ** BigInt(metadata.decimals!)).toString(),
		};
	}

	override async getMoveFunctionArgTypes(
		input: GetMoveFunctionArgTypesParams,
	): Promise<SuiMoveFunctionArgType[]> {
		const moveModule = await this.#graphqlQuery(
			{
				query: GetMoveFunctionArgTypesDocument,
				variables: {
					module: input.module,
					packageId: input.package,
					function: input.function,
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
	}

	override async getNormalizedMoveFunction(
		input: GetNormalizedMoveFunctionParams,
	): Promise<SuiMoveNormalizedFunction> {
		const moveFunction = await this.#graphqlQuery(
			{
				query: GetNormalizedMoveFunctionDocument,
				variables: {
					module: input.module,
					packageId: input.package,
					function: input.function,
				},
			},
			(data) => data.object?.asMovePackage?.module?.function,
		);

		return mapNormalizedMoveFunction(moveFunction);
	}

	override async getNormalizedMoveModulesByPackage(
		input: GetNormalizedMoveModulesByPackageParams,
	): Promise<SuiMoveNormalizedModules> {
		const movePackage = await this.#graphqlQuery(
			{
				query: GetNormalizedMoveModulesByPackageDocument,
				variables: {
					packageId: input.package,
				},
			},
			(data) => data.object?.asMovePackage,
		);

		const address = toShortTypeString(movePackage.address);
		const modules: Record<string, SuiMoveNormalizedModule> = {};

		movePackage.modules?.nodes.forEach((module) => {
			modules[module.name] = mapNormalizedMoveModule(module, address);
		});

		return modules;
	}

	override async getNormalizedMoveModule(
		input: GetNormalizedMoveModuleParams,
	): Promise<SuiMoveNormalizedModule> {
		const moveModule = await this.#graphqlQuery(
			{
				query: GetNormalizedMoveModuleDocument,
				variables: {
					module: input.module,
					packageId: input.package,
				},
			},
			(data) => data.object?.asMovePackage?.module,
		);

		return mapNormalizedMoveModule(moveModule, input.package);
	}

	override async getNormalizedMoveStruct(
		input: GetNormalizedMoveStructParams,
	): Promise<SuiMoveNormalizedStruct> {
		const moveStruct = await this.#graphqlQuery(
			{
				query: GetNormalizedMoveStructDocument,
				variables: {
					module: input.module,
					packageId: input.package,
					struct: input.struct,
				},
			},
			(data) => data.object?.asMovePackage?.module?.struct,
		);

		return mapNormalizedMoveStruct(moveStruct);
	}

	override async getOwnedObjects(input: GetOwnedObjectsParams): Promise<PaginatedObjectsResponse> {
		const filter: ObjectFilter | null | undefined = input.filter && {
			objectIds:
				'ObjectIds' in input.filter
					? input.filter.ObjectIds
					: 'ObjectId' in input.filter
					? [input.filter.ObjectId]
					: undefined,
			type: 'StructType' in input.filter ? input.filter.StructType : undefined,
			owner:
				'ObjectOwner' in input.filter
					? input.filter.ObjectOwner
					: 'AddressOwner' in input.filter
					? input.filter.AddressOwner
					: undefined,
		};

		const unsupportedFilters = [
			'MatchAll',
			'MatchAny',
			'MatchNone',
			'Package',
			'MoveModule',
			'Version',
		];

		if (input.filter) {
			for (const unsupportedFilter of unsupportedFilters) {
				if (unsupportedFilter in input.filter) {
					throw new Error(`Filter ${unsupportedFilter} is not supported in GraphQL API`);
				}
			}
		}

		const { nodes: objects, pageInfo } = await this.#graphqlQuery(
			{
				query: GetOwnedObjectsDocument,
				variables: {
					owner: input.owner,
					limit: input.limit,
					cursor: input.cursor,
					showBcs: input.options?.showBcs,
					showContent: input.options?.showContent,
					showOwner: input.options?.showOwner,
					showPreviousTransaction: input.options?.showPreviousTransaction,
					showStorageRebate: input.options?.showStorageRebate,
					showType: input.options?.showType,
					filter,
				},
			},
			(data) => data.address?.objects,
		);

		return {
			hasNextPage: pageInfo.hasNextPage,
			nextCursor: pageInfo.endCursor,
			data: objects.map((object) => ({
				data: mapGraphQLMoveObjectToRpcObject(object, input.options ?? {}),
			})),
		};
	}

	override async getObject(input: GetObjectParams): Promise<SuiObjectResponse> {
		const object = await this.#graphqlQuery(
			{
				query: GetObjectDocument,
				variables: {
					id: input.id,
					showBcs: input.options?.showBcs,
					showContent: input.options?.showContent,
					showOwner: input.options?.showOwner,
					showPreviousTransaction: input.options?.showPreviousTransaction,
					showStorageRebate: input.options?.showStorageRebate,
					showType: input.options?.showType,
				},
			},
			(data) => data.object,
		);

		return {
			data: mapGraphQLObjectToRpcObject(object, input.options ?? {}),
		};
	}

	override async tryGetPastObject(input: TryGetPastObjectParams): Promise<ObjectRead> {
		const object = await this.#graphqlQuery(
			{
				query: TryGetPastObjectDocument,
				variables: {
					id: input.id,
					version: input.version,
					showBcs: input.options?.showBcs,
					showContent: input.options?.showContent,
					showOwner: input.options?.showOwner,
					showPreviousTransaction: input.options?.showPreviousTransaction,
					showStorageRebate: input.options?.showStorageRebate,
					showType: input.options?.showType,
				},
			},
			(data) => data.object,
		);

		// TODO: needs custom error handling

		return {
			status: 'VersionFound',
			details: mapGraphQLObjectToRpcObject(object, input.options ?? {}),
		};
	}

	override async multiGetObjects(input: MultiGetObjectsParams): Promise<SuiObjectResponse[]> {
		const objects = await this.#graphqlQuery(
			{
				query: MultiGetObjectsDocument,
				variables: {
					ids: input.ids,
					showBcs: input.options?.showBcs,
					showContent: input.options?.showContent,
					showOwner: input.options?.showOwner,
					showPreviousTransaction: input.options?.showPreviousTransaction,
					showStorageRebate: input.options?.showStorageRebate,
					showType: input.options?.showType,
					limit: input.ids.length,
				},
			},
			(data) => data.objects?.nodes,
		);

		return objects.map((object) => ({
			data: mapGraphQLObjectToRpcObject(object, input.options ?? {}),
		}));
	}

	override async queryTransactionBlocks(
		input: QueryTransactionBlocksParams,
	): Promise<PaginatedTransactionResponse> {
		const limit = input.limit ?? 20;
		const pagination =
			input.order === 'descending'
				? {
						last: limit,
						before: input.cursor,
				  }
				: {
						first: limit,
						after: input.cursor,
				  };

		const { nodes: transactionBlocks, pageInfo } = await this.#graphqlQuery(
			{
				query: QueryTransactionBlocksDocument,
				variables: {
					...pagination,
					showBalanceChanges: input.options?.showBalanceChanges,
					showEffects: input.options?.showEffects,
					showObjectChanges: input.options?.showObjectChanges,
					showRawInput: input.options?.showRawInput,
					showInput: input.options?.showInput,
					filter: input.filter
						? {
								atCheckpoint:
									'Checkpoint' in input.filter
										? Number.parseInt(input.filter.Checkpoint)
										: undefined,
								function:
									'MoveFunction' in input.filter
										? `${input.filter.MoveFunction.package}::${input.filter.MoveFunction.module}::${input.filter.MoveFunction.function}`
										: undefined,
								inputObject: 'InputObject' in input.filter ? input.filter.InputObject : undefined,
								changedObject:
									'ChangedObject' in input.filter ? input.filter.ChangedObject : undefined,
								signAddress: 'FromAddress' in input.filter ? input.filter.FromAddress : undefined,
								recvAddress: 'ToAddress' in input.filter ? input.filter.ToAddress : undefined,
								// FromOrToAddress
								// FromAndToAddress
								kind:
									'TransactionKind' in input.filter
										? (input.filter.TransactionKind as TransactionBlockKindInput) // TODO: ensure this is formatted correctly
										: undefined,
								// TransactionKindIn
						  }
						: {},
				},
			},
			(data) => data.transactionBlocks,
		);

		if (pagination.last) {
			transactionBlocks.reverse();
		}

		return {
			hasNextPage: pagination.last ? pageInfo.hasPreviousPage : pageInfo.hasNextPage,
			nextCursor: pagination.last ? pageInfo.startCursor : pageInfo.endCursor,
			data: transactionBlocks.map((transactionBlock) =>
				mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, input.options),
			),
		};
	}

	override async getTransactionBlock(
		input: GetTransactionBlockParams,
	): Promise<SuiTransactionBlockResponse> {
		const transactionBlock = await this.#graphqlQuery(
			{
				query: GetTransactionBlockDocument,
				variables: {
					digest: input.digest,
					showBalanceChanges: input.options?.showBalanceChanges,
					showEffects: input.options?.showEffects,
					showObjectChanges: input.options?.showObjectChanges,
					showRawInput: input.options?.showRawInput,
					showInput: input.options?.showInput,
				},
			},
			(data) => data.transactionBlock,
		);

		return mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, input.options);
	}

	override async multiGetTransactionBlocks(
		input: MultiGetTransactionBlocksParams,
	): Promise<SuiTransactionBlockResponse[]> {
		const transactionBlocks = await this.#graphqlQuery(
			{
				query: MultiGetTransactionBlocksDocument,
				variables: {
					digests: input.digests,
					showBalanceChanges: input.options?.showBalanceChanges,
					showEffects: input.options?.showEffects,
					showObjectChanges: input.options?.showObjectChanges,
					showRawInput: input.options?.showRawInput,
					showInput: input.options?.showInput,
					limit: input.digests.length,
				},
			},
			(data) => data.transactionBlocks?.nodes,
		);

		return transactionBlocks.map((transactionBlock) =>
			mapGraphQLTransactionBlockToRpcTransactionBlock(transactionBlock, input.options),
		);
	}

	override async getTotalTransactionBlocks(): Promise<bigint> {
		return this.#graphqlQuery(
			{
				query: GetTotalTransactionBlocksDocument,
			},
			(data) => BigInt(data.checkpoint?.networkTotalTransactions!),
		);
	}

	override async getReferenceGasPrice(): Promise<bigint> {
		const epoch = await this.#graphqlQuery(
			{
				query: GetReferenceGasPriceDocument,
				variables: {},
			},
			(data) => data.epoch,
		);

		return BigInt(epoch.referenceGasPrice);
	}

	override async getStakes(input: GetStakesParams): Promise<DelegatedStake[]> {
		const stakes = await this.#graphqlQuery(
			{
				query: GetStakesDocument,
				variables: {
					owner: input.owner,
				},
			},
			(data) => data.address?.stakedSuis?.nodes,
		);

		// TODO: need to figure out mapping to groups
		void stakes;
		throw new Error('Method not implemented.');
	}

	override async getStakesByIds(input: GetStakesByIdsParams): Promise<DelegatedStake[]> {
		const stakes = await this.#graphqlQuery(
			{
				query: GetStakesByIdsDocument,
				variables: {
					ids: input.stakedSuiIds,
				},
			},
			(data) => data.objects?.nodes.map((node) => node?.asMoveObject?.asStakedSui!).filter(Boolean),
		);

		// TODO: need to extract some details from contents
		void stakes;
		throw new Error('Method not implemented.');
	}

	override async getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
		const systemState = await this.#graphqlQuery(
			{
				query: GetLatestSuiSystemStateDocument,
			},
			(data) => data.epoch,
		);

		return {
			activeValidators: systemState.validatorSet?.activeValidators?.map(
				mapGraphQlValidatorToRpcValidator,
			)!,
			atRiskValidators: systemState.validatorSet?.activeValidators
				?.filter((validator) => validator.atRisk)
				.map((validator) => [validator.address.address!, validator.atRisk!.toString()])!,
			epoch: String(systemState.epochId),
			epochDurationMs: String(
				new Date(systemState.endTimestamp).getTime() -
					new Date(systemState.startTimestamp).getTime(),
			),
			epochStartTimestampMs: String(new Date(systemState.startTimestamp).getTime()),
			inactivePoolsId: 'TODO',
			inactivePoolsSize: String(systemState.validatorSet?.inactivePoolsSize),
			maxValidatorCount: String(systemState.systemParameters?.maxValidatorCount),
			minValidatorJoiningStake: String(systemState.systemParameters?.minValidatorJoiningStake),
			pendingActiveValidatorsId: 'TODO',
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
			stakingPoolMappingsId: 'TODO',
			stakingPoolMappingsSize: String(systemState.validatorSet?.stakePoolMappingsSize),
			storageFundNonRefundableBalance: String(systemState.storageFund?.nonRefundableBalance),
			storageFundTotalObjectStorageRebates: String(
				systemState.storageFund?.totalObjectStorageRebates,
			),
			systemStateVersion: String(systemState.systemStateVersion),
			totalStake: systemState.validatorSet?.totalStake,
			validatorCandidatesId: 'TODO',
			validatorCandidatesSize: systemState.validatorSet?.validatorCandidatesSize?.toString()!,
			validatorLowStakeGracePeriod: systemState.systemParameters?.validatorLowStakeGracePeriod,
			validatorLowStakeThreshold: systemState.systemParameters?.validatorLowStakeThreshold,
			validatorReportRecords: systemState.validatorSet?.activeValidators?.flatMap(
				(validator) => validator.reportRecords?.map((record) => record.address)!,
			)!,
			validatorVeryLowStakeThreshold: systemState.systemParameters?.validatorVeryLowStakeThreshold,
		};
	}

	override async queryEvents(input: QueryEventsParams): Promise<PaginatedEvents> {
		const pagination: Partial<QueryEventsQueryVariables> =
			input.order === 'ascending'
				? { first: input.limit, after: input.cursor as never }
				: { last: input.limit, before: input.cursor as never };

		const filter: QueryEventsQueryVariables['filter'] = {
			sender: 'Sender' in input.query ? input.query.Sender : undefined,
			transactionDigest: 'Transaction' in input.query ? input.query.Transaction : undefined,
			eventType: 'MoveEventType' in input.query ? input.query.MoveEventType : undefined,
			emittingModule:
				'MoveModule' in input.query
					? `${input.query.MoveModule.package}::${input.query.MoveModule.module}`
					: undefined,
		};

		if ('Package' in input.query) {
			throw new Error('querying events by Package is not supported in the GraphQL API');
		}

		if ('MoveEventModule' in input.query) {
			throw new Error('querying events by MoveEventModule is not supported in the GraphQL API');
		}

		const { nodes: events, pageInfo } = await this.#graphqlQuery(
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
				id: 'TODO' as never, // TODO: turn id into an object
				packageId: event.sendingModule?.package.address!,
				parsedJson: event.json ? JSON.parse(event.json) : undefined,
				sender: event.sender?.address,
				timestampMs: new Date(event.timestamp).getTime().toString(),
				transactionModule: 'TODO',
				type: toShortTypeString(event.type?.repr)!,
			})),
		};
	}

	override async devInspectTransactionBlock(
		input: DevInspectTransactionBlockParams,
	): Promise<DevInspectResults> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async getDynamicFields(input: GetDynamicFieldsParams): Promise<DynamicFieldPage> {
		const { nodes: fields, pageInfo } = await this.#graphqlQuery(
			{
				query: GetDynamicFieldsDocument,
				variables: {
					parentId: input.parentId,
					first: input.limit,
					cursor: input.cursor,
				},
			},
			(data) => data.object?.dynamicFields,
		);

		return {
			data: fields.map((field) => ({
				bcsName: field.name?.bcs,
				digest: (field.value?.__typename === 'MoveObject' ? field.value.digest : undefined)!,
				name: {
					type: toShortTypeString(field.name?.type.repr)!,
					value: field.name?.json.bytes,
				},
				objectId: field.value?.__typename === 'MoveObject' ? field.value.address : undefined,
				objectType: (field.value?.__typename === 'MoveObject'
					? field.value.contents?.type.repr
					: undefined)!,
				type: field.value?.__typename === 'MoveObject' ? 'DynamicObject' : 'DynamicField',
				version: (field.value?.__typename === 'MoveObject'
					? field.value.version
					: undefined) as unknown as string, // RPC types are wrong here,
			})),
			nextCursor: pageInfo.endCursor ?? null,
			hasNextPage: pageInfo.hasNextPage,
		};
	}

	override async getDynamicFieldObject(
		input: GetDynamicFieldObjectParams,
	): Promise<SuiObjectResponse> {
		const field = await this.#graphqlQuery(
			{
				query: GetDynamicFieldObjectDocument,
				variables: {
					parentId: input.parentId,
					name: {
						type: input.name.type,
						bcs: input.name.value,
					},
				},
			},
			(data) => {
				return data.object?.dynamicObjectField;
			},
		);

		if (field.value?.__typename !== 'MoveObject') {
			throw new Error('Expected a MoveObject');
		}

		return {
			data: {
				content: {
					dataType: 'moveObject' as const,
					fields: moveDataToRpcContent(
						field.value.contents?.data!,
						field.value?.contents?.type.layout!,
					) as MoveStruct,
					hasPublicTransfer: field.value.hasPublicTransfer!,
					type: toShortTypeString(field.value.contents?.type.repr!),
				},
				digest: field.value.digest!,
				display: formatDisplay(field.value),
				objectId: field.value.address,
				type: toShortTypeString(field.value.contents?.type.repr),
				version: field.value.version as unknown as string, // RPC types are wrong here
			},
		};
	}
	override async subscribeEvent(
		input: SubscribeEventParams & { onMessage: (event: SuiEvent) => void },
	): Promise<Unsubscribe> {
		void input;
		throw new Error('Method not implemented.');
	}
	override async subscribeTransaction(
		input: SubscribeTransactionParams & { onMessage: (event: TransactionEffects) => void },
	): Promise<Unsubscribe> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async executeTransactionBlock(
		input: ExecuteTransactionBlockParams,
	): Promise<SuiTransactionBlockResponse> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async dryRunTransactionBlock(
		input: DryRunTransactionBlockParams,
	): Promise<DryRunTransactionBlockResponse> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async call<T = unknown>(method: string, params: unknown[]): Promise<T> {
		void method;
		void params;
		throw new Error('Method not implemented.');
	}

	override async getLatestCheckpointSequenceNumber(): Promise<string> {
		const sequenceNumber = await this.#graphqlQuery(
			{
				query: GetLatestCheckpointSequenceNumberDocument,
			},
			(data) => data.checkpoint?.sequenceNumber,
		);

		return sequenceNumber.toString();
	}

	override async getCheckpoint(input: GetCheckpointParams): Promise<Checkpoint> {
		const checkpoint = await this.#graphqlQuery(
			{
				query: GetCheckpointDocument,
				variables: {
					id:
						typeof input.id === 'number' || isNumericString(input.id)
							? {
									sequenceNumber: Number.parseInt(input.id.toString(), 10),
							  }
							: {
									digest: input.id,
							  },
				},
			},
			(data) => data.checkpoint,
		);

		return {
			checkpointCommitments: [], // TODO
			digest: checkpoint.digest,
			// endOfEpochData: checkpoint.endOfEpoch && { // TODO
			// 	epochCommitments: [], // TODO
			// 	nextEpochCommittee: [], // TODO
			// 	nextEpochProtocolVersion: String(checkpoint.endOfEpoch.nextProtocolVersion),
			// },
			epoch: String(checkpoint.epoch?.epochId),
			epochRollingGasCostSummary: {
				computationCost: checkpoint.rollingGasSummary?.computationCost,
				nonRefundableStorageFee: checkpoint.rollingGasSummary?.nonRefundableStorageFee,
				storageCost: checkpoint.rollingGasSummary?.storageCost,
				storageRebate: checkpoint.rollingGasSummary?.storageRebate,
			},
			networkTotalTransactions: String(checkpoint.networkTotalTransactions),
			...(checkpoint.previousCheckpointDigest
				? { previousDigest: checkpoint.previousCheckpointDigest }
				: {}),
			sequenceNumber: String(checkpoint.sequenceNumber),
			timestampMs: new Date(checkpoint.timestamp).getTime().toString(),
			transactions:
				checkpoint.transactionBlocks?.nodes.map((transactionBlock) => transactionBlock.digest!) ??
				[],
			validatorSignature: checkpoint.validatorSignatures,
		};
	}
	override async getCheckpoints(
		input: PaginationArguments<string | null> & GetCheckpointsParams,
	): Promise<CheckpointPage> {
		const pagination: Partial<QueryEventsQueryVariables> = input.descendingOrder
			? { last: input.limit, before: input.cursor as never }
			: { first: input.limit, after: input.cursor as never };

		const { nodes: checkpoints, pageInfo } = await this.#graphqlQuery(
			{
				query: GetCheckpointsDocument,
				variables: {
					...pagination,
				},
			},
			(data) => data.checkpoints,
		);

		if (pagination.last) {
			checkpoints.reverse();
		}

		return {
			hasNextPage: pagination.last ? pageInfo.hasPreviousPage : pageInfo.hasNextPage,
			nextCursor: (pagination.last ? pageInfo.startCursor : pageInfo.endCursor) as never,
			data: checkpoints.map((checkpoint) => ({
				checkpointCommitments: [], // TODO
				digest: checkpoint.digest,
				// endOfEpochData: checkpoint.endOfEpoch && { // TODO
				// 	epochCommitments: [], // TODO
				// 	nextEpochCommittee: [], // TODO
				// 	nextEpochProtocolVersion: String(checkpoint.endOfEpoch.nextProtocolVersion),
				// },
				epoch: String(checkpoint.epoch?.epochId),
				epochRollingGasCostSummary: {
					computationCost: checkpoint.rollingGasSummary?.computationCost,
					nonRefundableStorageFee: checkpoint.rollingGasSummary?.nonRefundableStorageFee,
					storageCost: checkpoint.rollingGasSummary?.storageCost,
					storageRebate: checkpoint.rollingGasSummary?.storageRebate,
				},
				networkTotalTransactions: String(checkpoint.networkTotalTransactions),
				...(checkpoint.previousCheckpointDigest
					? { previousDigest: checkpoint.previousCheckpointDigest }
					: {}),
				sequenceNumber: String(checkpoint.sequenceNumber),
				timestampMs: new Date(checkpoint.timestamp).getTime().toString(),
				transactions:
					checkpoint.transactionBlocks?.nodes.map((transactionBlock) => transactionBlock.digest!) ??
					[],
				validatorSignature: checkpoint.validatorSignatures,
			})),
		};
	}

	override async getCommitteeInfo(
		input?: GetCommitteeInfoParams | undefined,
	): Promise<CommitteeInfo> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async getNetworkMetrics(): Promise<NetworkMetrics> {
		throw new Error('Method not implemented.');
	}

	override async getMoveCallMetrics(): Promise<MoveCallMetrics> {
		throw new Error('Method not implemented.');
	}

	override async getAddressMetrics(): Promise<AddressMetrics> {
		throw new Error('Method not implemented.');
	}

	override async getAllEpochAddressMetrics(
		input?: { descendingOrder?: boolean | undefined } | undefined,
	): Promise<AllEpochsAddressMetrics> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async getEpochs(
		input?:
			| ({ descendingOrder?: boolean | undefined } & PaginationArguments<string | null>)
			| undefined,
	): Promise<EpochPage> {
		void input;
		throw new Error('Method not implemented.');
	}

	override async getCurrentEpoch(): Promise<EpochInfo> {
		const epoch = await this.#graphqlQuery(
			{
				query: GetCurrentEpochDocument,
			},
			(data) => data.epoch,
		);

		return {
			epoch: String(epoch.epochId),
			validators: epoch.validatorSet?.activeValidators?.map(mapGraphQlValidatorToRpcValidator)!,
			epochTotalTransactions: 'TODO',
			firstCheckpointId: epoch.firstCheckpoint?.nodes[0]?.sequenceNumber.toString()!,
			endOfEpochInfo: null,
			referenceGasPrice: Number.parseInt(epoch.referenceGasPrice, 10),
			epochStartTimestamp: new Date(epoch.startTimestamp).getTime().toString(),
		};
	}

	override async getValidatorsApy(): Promise<ValidatorsApy> {
		const epoch = await this.#graphqlQuery(
			{
				query: GetValidatorsApyDocument,
			},
			(data) => data.epoch,
		);

		return {
			epoch: String(epoch.epochId),
			apys: epoch.validatorSet?.activeValidators?.map((validator) => ({
				address: validator.address.address!,
				apy: (validator.apy ? validator.apy / 100 : null) as number,
			}))!,
		};
	}

	override async getChainIdentifier(): Promise<string> {
		const identifier = await this.#graphqlQuery(
			{
				query: GetChainIdentifierDocument,
			},
			(data) => data.chainIdentifier,
		);

		return identifier;
	}

	override async getProtocolConfig(
		input?: GetProtocolConfigParams | undefined,
	): Promise<ProtocolConfig> {
		const protocolConfig = await this.#graphqlQuery(
			{
				query: GetProtocolConfigDocument,
				variables: {
					protocolVersion: input?.version ? Number.parseInt(input.version) : undefined,
				},
			},
			(data) => data.protocolConfig,
		);

		const featureFlags: Record<string, boolean> = {};
		const attributes: Record<string, ProtocolConfigValue | null> = {};

		const configTypeMap: Record<string, string> = {
			max_arguments: 'u32',
			max_gas_payment_objects: 'u32',
			max_modules_in_publish: 'u32',
			max_programmable_tx_commands: 'u32',
			max_pure_argument_size: 'u32',
			max_type_argument_depth: 'u32',
			max_type_arguments: 'u32',
			move_binary_format_version: 'u32',
			random_beacon_reduction_allowed_delta: 'u16',
			scoring_decision_cutoff_value: 'f64',
			scoring_decision_mad_divisor: 'f64',
		};

		for (const { key, value } of protocolConfig.configs) {
			attributes[key] = {
				[configTypeMap[key] ?? 'u64']: value,
			} as ProtocolConfigValue;
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
	}

	override async resolveNameServiceAddress(
		input: ResolveNameServiceAddressParams,
	): Promise<string | null> {
		const data = await this.#graphqlQuery({
			query: ResolveNameServiceAddressDocument,
			variables: {
				domain: input.name,
			},
		});

		return data.resolveSuinsAddress?.address ?? null;
	}

	override async resolveNameServiceNames(
		input: ResolveNameServiceNamesParams,
	): Promise<ResolvedNameServiceNames> {
		const address = await this.#graphqlQuery(
			{
				query: ResolveNameServiceNamesDocument,
				variables: {
					address: input.address,
				},
			},
			(data) => data.address,
		);

		return {
			hasNextPage: false,
			nextCursor: null,
			data: address.suinsRegistrations?.nodes.map((node) => node.domain) ?? [],
		};
	}
}

function handleGraphQLErrors(errors: GraphQLResponseErrors | undefined): void {
	if (!errors || errors.length === 0) return;

	const errorInstances = errors.map((error) => new GraphQLResponseError(error));

	if (errorInstances.length === 1) {
		throw errorInstances[0];
	}

	throw new AggregateError(errorInstances);
}

class GraphQLResponseError extends Error {
	locations?: Array<{ line: number; column: number }>;

	constructor(error: GraphQLResponseErrors[0]) {
		super(error.message);
		this.locations = error.locations;
	}
}
