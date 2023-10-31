// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TypedDocumentNode } from '@graphql-typed-document-node/core';

import type { PaginationArguments } from '../client/client.js';
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
	ObjectRead,
	PaginatedCoins,
	PaginatedEvents,
	PaginatedObjectsResponse,
	PaginatedTransactionResponse,
	ProtocolConfig,
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

export class GraphQLSuiClient extends SuiClient {
	graphqlQuery<T extends TypedDocumentNode | string>(options: { query: T }): unknown {}

	override getRpcApiVersion(): Promise<string | undefined> {
		throw new Error('Method not implemented.');
	}

	override getCoins(input: GetCoinsParams): Promise<PaginatedCoins> {
		throw new Error('Method not implemented.');
	}

	override getAllCoins(input: GetAllCoinsParams): Promise<PaginatedCoins> {
		return this.getCoins({
			...input,
			coinType: null,
		});
	}
	override getBalance(input: GetBalanceParams): Promise<CoinBalance> {
		throw new Error('Method not implemented.');
	}
	override getAllBalances(input: GetAllBalancesParams): Promise<CoinBalance[]> {
		throw new Error('Method not implemented.');
	}

	override getCoinMetadata(input: GetCoinMetadataParams): Promise<CoinMetadata | null> {
		throw new Error('Method not implemented.');
	}

	override getTotalSupply(input: GetTotalSupplyParams): Promise<CoinSupply> {
		throw new Error('Method not implemented.');
	}

	override getMoveFunctionArgTypes(
		input: GetMoveFunctionArgTypesParams,
	): Promise<SuiMoveFunctionArgType[]> {
		throw new Error('Method not implemented.');
	}

	override getNormalizedMoveFunction(
		input: GetNormalizedMoveFunctionParams,
	): Promise<SuiMoveNormalizedFunction> {
		throw new Error('Method not implemented.');
	}

	override getNormalizedMoveModulesByPackage(
		input: GetNormalizedMoveModulesByPackageParams,
	): Promise<SuiMoveNormalizedModules> {
		throw new Error('Method not implemented.');
	}

	override getNormalizedMoveModule(
		input: GetNormalizedMoveModuleParams,
	): Promise<SuiMoveNormalizedModule> {
		const query = /* GraphQL */ `
			query getNormalizedMoveModule($packageId: SuiAddress!, $module: String!) {
				object(address: $packageId) {
					asMovePackage {
						module(name: $module) {
							fileFormatVersion
							# Missing function definitions
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override getNormalizedMoveStruct(
		input: GetNormalizedMoveStructParams,
	): Promise<SuiMoveNormalizedStruct> {
		throw new Error('Method not implemented.');
	}

	override getOwnedObjects(input: GetOwnedObjectsParams): Promise<PaginatedObjectsResponse> {
		throw new Error('Method not implemented.');
	}

	override getObject(input: GetObjectParams): Promise<SuiObjectResponse> {
		throw new Error('Method not implemented.');
	}

	override tryGetPastObject(input: TryGetPastObjectParams): Promise<ObjectRead> {
		throw new Error('Method not implemented.');
	}

	override multiGetObjects(input: MultiGetObjectsParams): Promise<SuiObjectResponse[]> {
		throw new Error('Method not implemented.');
	}

	override queryTransactionBlocks(
		input: QueryTransactionBlocksParams,
	): Promise<PaginatedTransactionResponse> {
		throw new Error('Method not implemented.');
	}

	override getTransactionBlock(
		input: GetTransactionBlockParams,
	): Promise<SuiTransactionBlockResponse> {
		throw new Error('Method not implemented.');
	}

	override multiGetTransactionBlocks(
		input: MultiGetTransactionBlocksParams,
	): Promise<SuiTransactionBlockResponse[]> {
		throw new Error('Method not implemented.');
	}

	override getTotalTransactionBlocks(): Promise<bigint> {
		throw new Error('Method not implemented.');
	}

	override getReferenceGasPrice(): Promise<bigint> {
		throw new Error('Method not implemented.');
	}
	override getStakes(input: GetStakesParams): Promise<DelegatedStake[]> {
		throw new Error('Method not implemented.');
	}

	override getStakesByIds(input: GetStakesByIdsParams): Promise<DelegatedStake[]> {
		throw new Error('Method not implemented.');
	}
	override getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
		throw new Error('Method not implemented.');
	}

	override queryEvents(input: QueryEventsParams): Promise<PaginatedEvents> {
		throw new Error('Method not implemented.');
	}

	override devInspectTransactionBlock(
		input: DevInspectTransactionBlockParams,
	): Promise<DevInspectResults> {
		throw new Error('Method not implemented.');
	}

	override getDynamicFields(input: GetDynamicFieldsParams): Promise<DynamicFieldPage> {
		throw new Error('Method not implemented.');
	}

	override getDynamicFieldObject(input: GetDynamicFieldObjectParams): Promise<SuiObjectResponse> {
		throw new Error('Method not implemented.');
	}
	override subscribeEvent(
		input: SubscribeEventParams & { onMessage: (event: SuiEvent) => void },
	): Promise<Unsubscribe> {
		throw new Error('Method not implemented.');
	}
	override subscribeTransaction(
		input: SubscribeTransactionParams & { onMessage: (event: TransactionEffects) => void },
	): Promise<Unsubscribe> {
		throw new Error('Method not implemented.');
	}

	override executeTransactionBlock(
		input: ExecuteTransactionBlockParams,
	): Promise<SuiTransactionBlockResponse> {
		throw new Error('Method not implemented.');
	}

	override dryRunTransactionBlock(
		input: DryRunTransactionBlockParams,
	): Promise<DryRunTransactionBlockResponse> {
		throw new Error('Method not implemented.');
	}

	override call<T = unknown>(method: string, params: unknown[]): Promise<T> {
		throw new Error('Method not implemented.');
	}

	override getLatestCheckpointSequenceNumber(): Promise<string> {
		throw new Error('Method not implemented.');
	}

	override getCheckpoint(input: GetCheckpointParams): Promise<Checkpoint> {
		throw new Error('Method not implemented.');
	}
	override getCheckpoints(
		input: PaginationArguments<string | null> & GetCheckpointsParams,
	): Promise<CheckpointPage> {
		throw new Error('Method not implemented.');
	}

	override getCommitteeInfo(input?: GetCommitteeInfoParams | undefined): Promise<CommitteeInfo> {
		// not sure if this is available
		throw new Error('Method not implemented.');
	}

	override getNetworkMetrics(): Promise<NetworkMetrics> {
		throw new Error('Method not implemented.');
	}

	override getMoveCallMetrics(): Promise<MoveCallMetrics> {
		throw new Error('Method not implemented.');
	}

	override getAddressMetrics(): Promise<AddressMetrics> {
		throw new Error('Method not implemented.');
	}

	override getAllEpochAddressMetrics(
		input?: { descendingOrder?: boolean | undefined } | undefined,
	): Promise<AllEpochsAddressMetrics> {
		throw new Error('Method not implemented.');
	}

	override getEpochs(
		input?:
			| ({ descendingOrder?: boolean | undefined } & PaginationArguments<string | null>)
			| undefined,
	): Promise<EpochPage> {
		throw new Error('Method not implemented.');
	}

	override getCurrentEpoch(): Promise<EpochInfo> {
		throw new Error('Method not implemented.');
	}

	override getValidatorsApy(): Promise<ValidatorsApy> {
		throw new Error('Method not implemented.');
	}

	override getChainIdentifier(): Promise<string> {
		throw new Error('Method not implemented.');
	}
	override getProtocolConfig(input?: GetProtocolConfigParams | undefined): Promise<ProtocolConfig> {
		throw new Error('Method not implemented.');
	}

	override resolveNameServiceAddress(
		input: ResolveNameServiceAddressParams,
	): Promise<string | null> {
		throw new Error('Method not implemented.');
	}

	override resolveNameServiceNames(
		input: ResolveNameServiceNamesParams,
	): Promise<ResolvedNameServiceNames> {
		throw new Error('Method not implemented.');
	}
}
