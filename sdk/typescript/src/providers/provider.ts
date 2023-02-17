// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SerializedSignature } from '../cryptography/signature';
import { HttpHeaders } from '../rpc/client';
import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import {
  GetObjectDataResponse,
  SuiObjectInfo,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  SuiObjectRef,
  SuiMoveFunctionArgTypes,
  SuiMoveNormalizedFunction,
  SuiMoveNormalizedStruct,
  SuiMoveNormalizedModule,
  SuiMoveNormalizedModules,
  SuiEventFilter,
  SuiEventEnvelope,
  SubscriptionId,
  ExecuteTransactionRequestType,
  SuiExecuteTransactionResponse,
  TransactionDigest,
  ObjectId,
  SuiAddress,
  EventQuery,
  EventId,
  PaginatedTransactionDigests,
  TransactionQuery,
  PaginatedEvents,
  RpcApiVersion,
  FaucetResponse,
  Order,
  TransactionEffects,
  CoinMetadata,
  DevInspectResults,
  SuiSystemState,
  DelegatedStake,
  ValidatorMetaData,
  PaginatedCoins,
  CoinBalance,
  CoinSupply,
  CheckpointSummary,
  CheckpointContents,
  CheckpointDigest,
  CheckPointContentsDigest,
  CommitteeInfo,
} from '../types';

import { DynamicFieldPage } from '../types/dynamic_fields';

///////////////////////////////
// Exported Abstracts
export abstract class Provider {
  // API Version
  /**
   * Fetch and cache the RPC API version number
   *
   * @return the current version of the RPC API that the provider is
   * connected to, or undefined if any error occurred
   */
  abstract getRpcApiVersion(): Promise<RpcApiVersion | undefined>;

  // Faucet
  /**
   * Request gas tokens from a faucet server
   * @param recipient the address for receiving the tokens
   * @param httpHeaders optional request headers
   */
  abstract requestSuiFromFaucet(
    recipient: SuiAddress,
    httpHeaders?: HttpHeaders,
  ): Promise<FaucetResponse>;

  // RPC Endpoint
  /**
   * Invoke any RPC endpoint
   * @param endpoint the endpoint to be invoked
   * @param params the arguments to be passed to the RPC request
   */
  abstract call(endpoint: string, params: Array<any>): Promise<any>;

  // Coins
  /**
   * Get all Coin<`coin_type`> objects owned by an address.
   * @param coinType optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
   * @param cursor optional paging cursor
   * @param limit maximum number of items per page
   */
  abstract getCoins(
    owner: SuiAddress,
    coinType: string | null,
    cursor: ObjectId | null,
    limit: number | null,
  ): Promise<PaginatedCoins>;

  /**
   * Get all Coin objects owned by an address.
   * @param cursor optional paging cursor
   * @param limt maximum number of items per page
   */
  abstract getAllCoins(
    owner: SuiAddress,
    cursor: ObjectId | null,
    limit: number | null,
  ): Promise<PaginatedCoins>;

  /**
   * Get the total coin balance for one coin type, owned by the address owner.
   * @param coinType optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
   */
  abstract getBalance(
    owner: SuiAddress,
    coinType: string | null,
  ): Promise<CoinBalance>;

  /**
   * Get the total coin balance for all coin type, owned by the address owner.
   */
  abstract getAllBalances(owner: SuiAddress): Promise<CoinBalance[]>;

  /**
   * Fetch CoinMetadata for a given coin type
   * @param coinType fully qualified type names for the coin (e.g.,
   * 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
   *
   */
  abstract getCoinMetadata(coinType: string): Promise<CoinMetadata>;

  /**
   *  Fetch total supply for a coin
   * @param coinType fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
   */
  abstract getTotalSupply(coinType: string): Promise<CoinSupply>;

  // Objects
  /**
   * Get all objects owned by an address
   */
  abstract getObjectsOwnedByAddress(
    addressOrObjectId: string,
  ): Promise<SuiObjectInfo[]>;

  /**
   * Convenience method for getting all gas objects(SUI Tokens) owned by an address
   */
  abstract getGasObjectsOwnedByAddress(
    _address: string,
  ): Promise<SuiObjectInfo[]>;

  /**
   * @deprecated The method should not be used
   */
  abstract getCoinBalancesOwnedByAddress(
    address: string,
    typeArg?: string,
  ): Promise<GetObjectDataResponse[]>;

  /**
   * Convenience method for select coin objects that has a balance greater than or equal to `amount`
   *
   * @param amount coin balance
   * @param typeArg coin type, e.g., '0x2::sui::SUI'
   * @param exclude object ids of the coins to exclude
   * @return a list of coin objects that has balance greater than `amount` in an ascending order
   */
  abstract selectCoinsWithBalanceGreaterThanOrEqual(
    address: string,
    amount: bigint,
    typeArg: string,
    exclude: ObjectId[],
  ): Promise<GetObjectDataResponse[]>;

  /**
   * Convenience method for select a minimal set of coin objects that has a balance greater than
   * or equal to `amount`. The output can be used for `PayTransaction`
   *
   * @param amount coin balance
   * @param typeArg coin type, e.g., '0x2::sui::SUI'
   * @param exclude object ids of the coins to exclude
   * @return a minimal list of coin objects that has a combined balance greater than or equal
   * to`amount` in an ascending order. If no such set exists, an empty list is returned
   */
  abstract selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    address: string,
    amount: bigint,
    typeArg: string,
    exclude: ObjectId[],
  ): Promise<GetObjectDataResponse[]>;

  /**
   * Get details about an object
   */
  abstract getObject(objectId: string): Promise<GetObjectDataResponse>;

  /**
   * Get object reference(id, tx digest, version id)
   * @param objectId
   */
  abstract getObjectRef(objectId: string): Promise<SuiObjectRef | undefined>;

  // Transactions
  /**
   * Get transaction digests for a given range
   *
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber,
  ): Promise<GetTxnDigestsResponse>;

  /**
   * Get transactions for a given query criteria
   */
  abstract getTransactions(
    query: TransactionQuery,
    cursor: TransactionDigest | null,
    limit: number | null,
    order: Order,
  ): Promise<PaginatedTransactionDigests>;

  /**
   * Get total number of transactions
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTotalTransactionNumber(): Promise<number>;

  /**
   * This is under development endpoint on Fullnode that will eventually
   * replace the other `executeTransaction` that's only available on the
   * Gateway
   */
  abstract executeTransaction(
    txnBytes: Uint8Array | string,
    signature: SerializedSignature,
    requestType: ExecuteTransactionRequestType,
  ): Promise<SuiExecuteTransactionResponse>;

  // Move info
  /**
   * Get Move function argument types like read, write and full access
   */
  abstract getMoveFunctionArgTypes(
    objectId: string,
    moduleName: string,
    functionName: string,
  ): Promise<SuiMoveFunctionArgTypes>;

  /**
   * Get a map from module name to
   * structured representations of Move modules
   */
  abstract getNormalizedMoveModulesByPackage(
    objectId: string,
  ): Promise<SuiMoveNormalizedModules>;

  /**
   * Get a structured representation of Move module
   */
  abstract getNormalizedMoveModule(
    objectId: string,
    moduleName: string,
  ): Promise<SuiMoveNormalizedModule>;

  /**
   * Get a structured representation of Move function
   */
  abstract getNormalizedMoveFunction(
    objectId: string,
    moduleName: string,
    functionName: string,
  ): Promise<SuiMoveNormalizedFunction>;

  /**
   * Get a structured representation of Move struct
   */
  abstract getNormalizedMoveStruct(
    objectId: string,
    moduleName: string,
    structName: string,
  ): Promise<SuiMoveNormalizedStruct>;

  /**
   * Get events for a given query criteria
   * @param query - the event query criteria.
   * @param cursor - optional paging cursor
   * @param limit - maximum number of items per page
   * @param order - event query ordering
   */
  abstract getEvents(
    query: EventQuery,
    cursor: EventId | null,
    limit: number | null,
    order: Order,
  ): Promise<PaginatedEvents>;

  /**
   * Subscribe to get notifications whenever an event matching the filter occurs
   * @param filter - filter describing the subset of events to follow
   * @param onMessage - function to run when we receive a notification of a new event matching the filter
   */
  abstract subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void,
  ): Promise<SubscriptionId>;

  /**
   * Unsubscribe from an event subscription
   * @param id - subscription id to unsubscribe from (previously received from subscribeEvent)
   */
  abstract unsubscribeEvent(id: SubscriptionId): Promise<boolean>;

  /**
   * Runs the transaction in dev-inpsect mode. Which allows for nearly any
   * transaction (or Move call) with any arguments. Detailed results are
   * provided, including both the transaction effects and any return values.
   *
   * @param sender the sender of the transaction
   * @param txn transaction without gasPayment, gasBudget, and gasPrice specified.
   * @param gas_price optional. Default to use the network reference gas price stored
   * in the Sui System State object
   * @param epoch optional. Default to use the current epoch number stored
   * in the Sui System State object
   */
  abstract devInspectTransaction(
    sender: SuiAddress,
    txn: UnserializedSignableTransaction | string | Uint8Array,
    gasPrice: number | null,
    epoch: number | null,
  ): Promise<DevInspectResults>;

  /**
   * Execute the transaction without committing any state changes on chain. This is useful for estimating
   * gas budget and the transaction effects
   * @param txBytes
   */
  abstract dryRunTransaction(txBytes: Uint8Array): Promise<TransactionEffects>;

  /**
   * Return the list of dynamic field objects owned by an object
   * @param parent_object_id - The id of the parent object
   * @param cursor - Optional paging cursor
   * @param limit - Maximum item returned per page
   */
  abstract getDynamicFields(
    parent_object_id: ObjectId,
    cursor: ObjectId | null,
    limit: number | null,
  ): Promise<DynamicFieldPage>;

  /**
   * Return the dynamic field object information for a specified object
   * @param parent_object_id - The ID of the quered parent object
   * @param name - The name of the dynamic field
   */
  abstract getDynamicFieldObject(
    parent_object_id: ObjectId,
    name: string,
  ): Promise<GetObjectDataResponse>;

  /**
   * Getting the reference gas price for the network
   */
  abstract getReferenceGasPrice(): Promise<number>;

  /**
   * Return the delegated stakes for an address
   */
  abstract getDelegatedStakes(address: SuiAddress): Promise<DelegatedStake[]>;

  /**
   * Return all validators available for stake delegation.
   */
  abstract getValidators(): Promise<ValidatorMetaData[]>;

  /**
   * Return the content of `0x5` object
   */
  abstract getSuiSystemState(): Promise<SuiSystemState>;

  /**
   * Get the sequence number of the latest checkpoint that has been executed
   */
  abstract getLatestCheckpointSequenceNumber(): Promise<number>;

  /**
   * Returns checkpoint summary based on a checkpoint sequence number
   * @param sequence_number - The sequence number of the desired checkpoint summary
   */
  abstract getCheckpointSummary(
    sequenceNumber: number,
  ): Promise<CheckpointSummary>;

  /**
   * Returns checkpoint summary based on a checkpoint digest
   * @param digest - The checkpoint digest
   */
  abstract getCheckpointSummaryByDigest(
    digest: CheckpointDigest,
  ): Promise<CheckpointSummary>;

  /**
   * Return contents of a checkpoint, namely a list of execution digests
   * @param sequence_number - The sequence number of the desired checkpoint contents
   */
  abstract getCheckpointContents(
    sequenceNumber: number,
  ): Promise<CheckpointContents>;

  /**
   * Returns checkpoint summary based on a checkpoint content digest
   * @param digest - The checkpoint summary digest
   */
  abstract getCheckpointContentsByDigest(
    digest: CheckPointContentsDigest,
  ): Promise<CheckpointContents>;

  /**
   * Return the committee information for the asked epoch
   * @param epoch The epoch of interest. If null, default to the latest epoch
   * @return {CommitteeInfo} the committee information
   */
  abstract getCommitteeInfo(epoch?: number): Promise<CommitteeInfo>;

  // TODO: add more interface methods
}
