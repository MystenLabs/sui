// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PaginationArguments } from './client.js';
import { SuiClient } from './client.js';
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
} from './types/chain.js';
import type { CoinBalance } from './types/coins.js';
import type { Unsubscribe } from './types/common.js';
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
} from './types/generated.js';
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
} from './types/params.js';

export class GraphQLSuiClient extends SuiClient {
	override getRpcApiVersion(): Promise<string | undefined> {
		throw new Error('Method not implemented.');
	}

	override getCoins(input: GetCoinsParams): Promise<PaginatedCoins> {
		const query = /* GraphQL */ `
			query getCoins(
				$owner: SuiAddress!
				$first: Int
				$cursor: String
				$type: String = "0x2::sui::SUI"
			) {
				address(address: $owner) {
					location
					coinConnection(first: $first, after: $cursor, type: $type) {
						pageInfo {
							hasNextPage
							endCursor
						}
						nodes {
							coinObjectId: id
							balance
							asMoveObject {
								contents {
									type {
										repr
									}
								}
								asObject {
									version
									digest

									previousTransactionBlock {
										digest
									}
								}
							}
						}
					}
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override getAllCoins(input: GetAllCoinsParams): Promise<PaginatedCoins> {
		return this.getCoins({
			...input,
			coinType: null,
		});
	}
	override getBalance(input: GetBalanceParams): Promise<CoinBalance> {
		const query = /* GraphQL */ `
			query getBalance($owner: SuiAddress!, $type: String = "0x2::sui::SUI") {
				address(address: $owner) {
					balance(type: $type) {
						coinType
						coinObjectCount
						totalBalance
						# lockedBalance not available in GraphQL
					}
				}
			}
		`;
		void query;

		throw new Error('Method not implemented.');
	}
	override getAllBalances(input: GetAllBalancesParams): Promise<CoinBalance[]> {
		const query = /* GraphQL */ `
			query getAllBalances($owner: SuiAddress!, $limit: Int, $cursor: String) {
				address(address: $owner) {
					balanceConnection(first: $limit, after: $cursor) {
						pageInfo {
							hasNextPage
							endCursor
						}
						nodes {
							coinType
							coinObjectCount
							totalBalance
							# lockedBalance not available in GraphQL
						}
					}
				}
			}
		`;
		void query;

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
		const query = /* GraphQL */ `
			query getMoveFunctionArgTypes($packageId: SuiAddress!, $module: String!) {
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

	override getNormalizedMoveFunction(
		input: GetNormalizedMoveFunctionParams,
	): Promise<SuiMoveNormalizedFunction> {
		const query = /* GraphQL */ `
			query getNormalizedMoveFunction($packageId: SuiAddress!, $module: String!) {
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

	override getNormalizedMoveModulesByPackage(
		input: GetNormalizedMoveModulesByPackageParams,
	): Promise<SuiMoveNormalizedModules> {
		const query = /* GraphQL */ `
			query getNormalizedMoveModulesByPackage(
				$packageId: SuiAddress!
				$limit: Int
				$cursor: String
			) {
				object(address: $packageId) {
					asMovePackage {
						moduleConnection(first: $limit, after: $cursor) {
							pageInfo {
								hasNextPage
								endCursor
							}
							nodes {
								fileFormatVersion
								# Missing function definitions
							}
						}
					}
				}
			}
		`;

		void query;
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

	override getOwnedObjects(input: GetOwnedObjectsParams): Promise<PaginatedObjectsResponse> {
		const query = /* GraphQL */ `
			query getOwnedObjects(
				$owner: SuiAddress!
				$limit: Int
				$cursor: String
				$showBcs: Boolean = false
				#$showContent: Boolean = false,
				#$showDisplay: Boolean = false,
				#$showType: Boolean = false
				$showOwner: Boolean = false
				$showPreviousTransaction: Boolean = false
				$showStorageRebate: Boolean = false
				$filter: ObjectFilter
			) {
				address(address: $owner) {
					# filter missing:
					# - version
					# - all, any, none
					# - struct type - has ty? option
					# - object owner vs address owner?
					objectConnection(first: $limit, after: $cursor, filter: $filter) {
						pageInfo {
							hasNextPage
							endCursor
						}
						nodes {
							objectId: location
							# bcs only partially supported
							bcs @include(if: $showBcs)
							# content not implemented
							# display not implemented
							# type not implemented

							owner @include(if: $showOwner) {
								location
							}
							previousTransactionBlock @include(if: $showPreviousTransaction) {
								digest
							}

							storageRebate @include(if: $showStorageRebate)
							digest
							version
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override getObject(input: GetObjectParams): Promise<SuiObjectResponse> {
		const query = /* GraphQL */ `
			query getObject(
				$id: SuiAddress!
				$showBcs: Boolean = false
				$showOwner: Boolean = false
				$showPreviousTransaction: Boolean = false
				#$showContent: Boolean = false,
				#$showDisplay: Boolean = false,
				#$showType: Boolean = false
				$showStorageRebate: Boolean = false
			) {
				object(address: $id) {
					objectId: location
					# bcs only partially supported
					bcs @include(if: $showBcs)
					# content not implemented
					# display not implemented
					# type not implemented
					owner @include(if: $showOwner) {
						location
					}
					previousTransactionBlock @include(if: $showPreviousTransaction) {
						digest
					}
					storageRebate @include(if: $showStorageRebate)
					digest
					version
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override tryGetPastObject(input: TryGetPastObjectParams): Promise<ObjectRead> {
		const query = /* GraphQL */ `
			query getObject(
				$id: SuiAddress!
				$version: Int
				$showBcs: Boolean = false
				$showOwner: Boolean = false
				$showPreviousTransaction: Boolean = false
				# content not implemented
				# display not implemented
				# type not implemented
				$showStorageRebate: Boolean = false
			) {
				object(address: $id) {
					objectId: location
					# bcs only partially supported
					bcs @include(if: $showBcs)
					# content not implemented
					# display not implemented
					# type not implemented
					owner @include(if: $showOwner) {
						location
					}
					previousTransactionBlock @include(if: $showPreviousTransaction) {
						digest
					}
					storageRebate @include(if: $showStorageRebate)
					digest
					version
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override multiGetObjects(input: MultiGetObjectsParams): Promise<SuiObjectResponse[]> {
		const query = /* GraphQL */ `
			query multiGetObjects(
				$ids: [SuiAddress!]!
				$limit: Int
				$cursor: String
				$showBcs: Boolean = false
				#$showContent: Boolean = false,
				#$showDisplay: Boolean = false,
				#$showType: Boolean = false
				$showOwner: Boolean = false
				$showPreviousTransaction: Boolean = false
				$showStorageRebate: Boolean = false
			) {
				objectConnection(first: $limit, after: $cursor, filter: { objectIds: $ids }) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						objectId: location
						# bcs only partially supported
						bcs @include(if: $showBcs)
						# content not implemented
						# display not implemented
						# type not implemented

						owner @include(if: $showOwner) {
							location
						}
						previousTransactionBlock @include(if: $showPreviousTransaction) {
							digest
						}

						storageRebate @include(if: $showStorageRebate)
						digest
						version
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override queryTransactionBlocks(
		input: QueryTransactionBlocksParams,
	): Promise<PaginatedTransactionResponse> {
		const query = /* GraphQL */ `
			query queryTransactionBlocks(
				$limit: Int
				$cursor: String
				$showBalanceChanges: Boolean = false
				$showEffects: Boolean = false
				#$showEvents: Boolean = false,
				#$showInput: Boolean = false,
				$showObjectChanges: Boolean = false
				$showRawInput: Boolean = false
				# missing order
				$filter: TransactionBlockFilter
			) {
				transactionBlockConnection(first: $limit, after: $cursor, filter: $filter) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						digest
						# timestampMs
						# checkpoint
						# events
						# transaction can be derived from bcs, except:
						# - valueType for Pure inputs
						# - requires a lot of re-structuring
						rawTransaction: bcs @include(if: $showRawInput)
						signatures {
							base64Sig
						}
						effects {
							balanceChanges @include(if: $showBalanceChanges) {
								# missing coinType
								owner {
									location
								}
								amount
							}
							# messageVersion
							# eventsDigest
							# object changes don't work, not sure if all data is available
							dependencies @include(if: $showEffects) {
								digest
							}
							status @include(if: $showEffects)
							gasEffects @include(if: $showEffects) {
								gasSummary {
									storageCost
									storageRebate
									nonRefundableStorageFee
									computationCost
								}
							}
							executedEpoch: epoch @include(if: $showEffects) {
								epochId
							}
							objectChanges @include(if: $showObjectChanges) {
								idCreated
								idDeleted
								inputState {
									version
									digest
									objectId: location
									owner {
										location
									}
								}
								outputState {
									version
									digest
									objectId: location
									owner {
										location
									}
								}
							}
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override getTransactionBlock(
		input: GetTransactionBlockParams,
	): Promise<SuiTransactionBlockResponse> {
		const query = /* GraphQL */ `
			query getTransactionBlock(
				$digest: String!
				$showBalanceChanges: Boolean = false
				$showEffects: Boolean = false
				#$showEvents: Boolean = false,
				#$showInput: Boolean = false,
				$showObjectChanges: Boolean = false
				$showRawInput: Boolean = false
			) {
				transactionBlock(digest: $digest) {
					digest
					# timestampMs
					# checkpoint
					# events
					# transaction can be derived from bcs, except:
					# - valueType for Pure inputs
					# - requires a lot of re-structuring
					rawTransaction: bcs @include(if: $showRawInput)
					signatures {
						base64Sig
					}
					effects {
						balanceChanges @include(if: $showBalanceChanges) {
							# missing coinType
							owner {
								location
							}
							amount
						}
						# messageVersion
						# eventsDigest
						# object changes don't work, not sure if all data is available
						dependencies @include(if: $showEffects) {
							digest
						}
						status @include(if: $showEffects)
						gasEffects @include(if: $showEffects) {
							gasSummary {
								storageCost
								storageRebate
								nonRefundableStorageFee
								computationCost
							}
						}
						executedEpoch: epoch @include(if: $showEffects) {
							epochId
						}
						objectChanges @include(if: $showObjectChanges) {
							idCreated
							idDeleted
							inputState {
								version
								digest
								objectId: location
								owner {
									location
								}
							}
							outputState {
								version
								digest
								objectId: location
								owner {
									location
								}
							}
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override multiGetTransactionBlocks(
		input: MultiGetTransactionBlocksParams,
	): Promise<SuiTransactionBlockResponse[]> {
		const query = /* GraphQL */ `
			query multiGetTransactionBlocks(
				$digests: [String!]!
				$limit: Int
				$cursor: String
				$showBalanceChanges: Boolean = false
				$showEffects: Boolean = false
				#$showEvents: Boolean = false,
				#$showInput: Boolean = false,
				$showObjectChanges: Boolean = false
				$showRawInput: Boolean = false
			) {
				transactionBlockConnection(
					first: $limit
					after: $cursor
					filter: { transactionIds: $digests }
				) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						digest
						# timestampMs
						# checkpoint
						# events
						# transaction can be derived from bcs, except:
						# - valueType for Pure inputs
						# - requires a lot of re-structuring
						rawTransaction: bcs @include(if: $showRawInput)
						signatures {
							base64Sig
						}
						effects {
							balanceChanges @include(if: $showBalanceChanges) {
								# missing coinType
								owner {
									location
								}
								amount
							}
							# messageVersion
							# eventsDigest
							# object changes don't work, not sure if all data is available
							dependencies @include(if: $showEffects) {
								digest
							}
							status @include(if: $showEffects)
							gasEffects @include(if: $showEffects) {
								gasSummary {
									storageCost
									storageRebate
									nonRefundableStorageFee
									computationCost
								}
							}
							executedEpoch: epoch @include(if: $showEffects) {
								epochId
							}
							objectChanges @include(if: $showObjectChanges) {
								idCreated
								idDeleted
								inputState {
									version
									digest
									objectId: location
									owner {
										location
									}
								}
								outputState {
									version
									digest
									objectId: location
									owner {
										location
									}
								}
							}
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override getTotalTransactionBlocks(): Promise<bigint> {
		throw new Error('Method not implemented.');
	}

	override getReferenceGasPrice(): Promise<bigint> {
		const query = /* GraphQL */ `
			query getReferenceGasPrice {
				epoch {
					referenceGasPrice
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}
	override getStakes(input: GetStakesParams): Promise<DelegatedStake[]> {
		const query = /* GraphQL */ `
			query getStakes($owner: SuiAddress!, $limit: Int, $cursor: String) {
				address(address: $owner) {
					# exceeds query cost?
					stakeConnection(first: $limit, after: $cursor) {
						pageInfo {
							hasNextPage
							endCursor
						}
						nodes {
							principal
							activeEpoch {
								epochId
							}
							requestEpoch {
								epochId
							}
							asMoveObject {
								# staking pool can be read from contents
								contents {
									json
								}
								# bad nesting
								asObject {
									location
								}
							}
							estimatedReward
							# validatorAddress?
							activeEpoch {
								referenceGasPrice
							}
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override getStakesByIds(input: GetStakesByIdsParams): Promise<DelegatedStake[]> {
		const query = /* GraphQL */ `
			query getStakesByIds($ids: [SuiAddress!]!, $limit: Int, $cursor: String) {
				objectConnection(first: $limit, after: $cursor, filter: { objectIds: $ids }) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						asMoveObject {
							asStake {
								principal
								activeEpoch {
									epochId
								}
								requestEpoch {
									epochId
								}
								asMoveObject {
									# staking pool can be read from contents
									contents {
										json
									}
									# bad nesting
									asObject {
										location
									}
								}
								estimatedReward
								# validatorAddress?
								activeEpoch {
									referenceGasPrice
								}
							}
						}
					}
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}
	override getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
		const query = /* GraphQL */ `
			query getLatestSuiSystemState {
				latestSuiSystemState {
					referenceGasPrice
					safeMode {
						enabled
						gasSummary {
							computationCost
							nonRefundableStorageFee
							storageCost
							storageRebate
						}
					}

					stakeSubsidy {
						balance
						currentDistributionAmount
						decreaseRate
						distributionCounter
						periodLength
						# stakeSubsidyStartEpoch
						# stakingPoolMappingsId
						# stakingPoolMappingsSize
					}

					storageFund {
						nonRefundableBalance
						totalObjectStorageRebates
					}
					systemStateVersion
					systemParameters {
						minValidatorCount
						maxValidatorCount
						minValidatorJoiningStake
						durationMs
						validatorLowStakeThreshold
						validatorLowStakeGracePeriod
						validatorVeryLowStakeThreshold
					}
					protocolConfigs {
						protocolVersion
					}
					validatorSet {
						activeValidators {
							...ValidatorFields
						}

						# atRiskValidators (missing number of epochs)
						# inactivePoolsId
						inactivePoolsSize
						# pendingActiveValidatorsId
						pendingActiveValidatorsSize
						# validatorCandidatesId
						validatorCandidatesSize
						pendingRemovals
						totalStake
					}

					epoch {
						epochId
						startTimestamp
						endTimestamp
					}
				}
			}

			fragment ValidatorFields on Validator {
				atRisk
				commissionRate
				exchangeRatesSize
				exchangeRates {
					asObject {
						location
					}
				}
				description
				gasPrice
				imageUrl
				name
				credentials {
					...CredentialFields
				}
				nextEpochCommissionRate
				nextEpochGasPrice
				nextEpochCredentials {
					...CredentialFields
				}
				nextEpochStake
				nextEpochCommissionRate
				operationCap {
					asObject {
						location
					}
				}
				pendingPoolTokenWithdraw
				pendingStake
				pendingTotalSuiWithdraw
				poolTokenBalance
				projectUrl
				rewardsPool
				stakingPoolSuiBalance
				address {
					location
				}
				votingPower
				reportRecords
			}

			fragment CredentialFields on ValidatorCredentials {
				netAddress
				networkPubKey
				p2PAddress
				primaryAddress
				workerPubKey
				workerAddress
				proofOfPossession
				protocolPubKey
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override queryEvents(input: QueryEventsParams): Promise<PaginatedEvents> {
		const query = /* GraphQL */ `
			query queryEvents(
				$filter: EventFilter!
				# filter missing:
				# - MoveEventField
				# - TimeRange
				# - All, Any, And, Or
				# missing order
				$limit: Int
				$cursor: String
			) {
				eventConnection(filter: $filter, first: $limit, after: $cursor) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						id
						sendingModuleId {
							package {
								asObject {
									location
								}
							}
							name
						}
						senders {
							location
						}
						eventType {
							repr
						}
						json
						bcs
						timestamp
					}
				}
			}
		`;
		void query;
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
		const query = /* GraphQL */ `
			query getLatestCheckpointSequenceNumber {
				checkpoint {
					sequenceNumber
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override getCheckpoint(input: GetCheckpointParams): Promise<Checkpoint> {
		const query = /* GraphQL */ `
			query getCheckpoint($id: CheckpointId) {
				checkpoint(id: $id) {
					# checkpointCommitments
					digest
					endOfEpoch {
						# epochCommitments
						newCommittee {
							authorityName
							stakeUnit
						}
						nextProtocolVersion
					}
					epoch {
						epochId
					}

					rollingGasSummary {
						computationCost
						storageCost
						storageRebate
						nonRefundableStorageFee
					}
					networkTotalTransactions
					previousCheckpointDigest
					sequenceNumber
					timestamp
					# might be truncated? should we set a higher limit or paginate?
					transactionBlockConnection {
						nodes {
							digest
						}
					}
					validatorSignature
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}
	override getCheckpoints(
		input: PaginationArguments<string | null> & GetCheckpointsParams,
	): Promise<CheckpointPage> {
		const query = /* GraphQL */ `
			query getCheckpoints(
				# missing order
				$limit: Int
				$cursor: String
			) {
				checkpointConnection(first: $limit, after: $cursor) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						# checkpointCommitments
						digest
						endOfEpoch {
							# epochCommitments
							newCommittee {
								authorityName
								stakeUnit
							}
							nextProtocolVersion
						}
						epoch {
							epochId
						}

						rollingGasSummary {
							computationCost
							storageCost
							storageRebate
							nonRefundableStorageFee
						}
						networkTotalTransactions
						previousCheckpointDigest
						sequenceNumber
						timestamp
						# migth be truncated? should we set a higher limit or paginate?
						transactionBlockConnection {
							nodes {
								digest
							}
						}
						validatorSignature
					}
				}
			}
		`;
		void query;
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
		const query = /* GraphQL */ `
			query getCurrentEpoch {
				epoch {
					epochId
					validatorSet {
						activeValidators {
							...ValidatorFields
						}
					}
					# epochTotalTransactions
					firstCheckpoint: checkpointConnection(first: 1) {
						nodes {
							digest
							sequenceNumber
						}
					}
					# missing end of epoch info
					lastCheckpoint: checkpointConnection(last: 1) {
						nodes {
							digest
							sequenceNumber

							endOfEpoch {
								nextProtocolVersion
							}
						}
					}
					startTimestamp
					endTimestamp
					referenceGasPrice
				}
			}

			fragment ValidatorFields on Validator {
				atRisk
				commissionRate
				exchangeRatesSize
				exchangeRates {
					asObject {
						location
					}
				}
				description
				gasPrice
				imageUrl
				name
				credentials {
					...CredentialFields
				}
				nextEpochCommissionRate
				nextEpochGasPrice
				nextEpochCredentials {
					...CredentialFields
				}
				nextEpochStake
				nextEpochCommissionRate
				operationCap {
					asObject {
						location
					}
				}
				pendingPoolTokenWithdraw
				pendingStake
				pendingTotalSuiWithdraw
				poolTokenBalance
				projectUrl
				rewardsPool
				stakingPoolSuiBalance
				address {
					location
				}
				votingPower
				reportRecords
			}

			fragment CredentialFields on ValidatorCredentials {
				netAddress
				networkPubKey
				p2PAddress
				primaryAddress
				workerPubKey
				workerAddress
				proofOfPossession
				protocolPubKey
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override getValidatorsApy(): Promise<ValidatorsApy> {
		throw new Error('Method not implemented.');
	}

	override getChainIdentifier(): Promise<string> {
		const query = /* GraphQL */ `
			query getChainIdentifier {
				chainIdentifier
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}
	override getProtocolConfig(input?: GetProtocolConfigParams | undefined): Promise<ProtocolConfig> {
		const query = /* GraphQL */ `
			query getProtocolConfig($protocolVersion: Int) {
				protocolConfig(protocolVersion: $protocolVersion) {
					protocolVersion
					configs {
						key
						value
					}
					featureFlags {
						key
						value
					}
					#maxSupportedProtocolVersion
					#minSupportedProtocolVersion
				}
			}
		`;

		void query;
		throw new Error('Method not implemented.');
	}

	override resolveNameServiceAddress(
		input: ResolveNameServiceAddressParams,
	): Promise<string | null> {
		const query = /* GraphQL */ `
			query resolveNameServiceAddress($name: String!) {
				resolveNameServiceAddress(name: $name) {
					location
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}

	override resolveNameServiceNames(
		input: ResolveNameServiceNamesParams,
	): Promise<ResolvedNameServiceNames> {
		// Querying for a wallet address seems to crash the service
		const query = /* GraphQL */ `
			query resolveNameServiceNames($address: SuiAddress!, $limit: Int, $cursor: String) {
				address(address: $address) {
					nameServiceConnection(first: $limit, after: $cursor) {
						pageInfo {
							hasNextPage
							endCursor
						}
						nodes
					}
				}
			}
		`;
		void query;
		throw new Error('Method not implemented.');
	}
}
