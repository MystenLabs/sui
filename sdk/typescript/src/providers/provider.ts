// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignatureScheme } from '../cryptography/publickey';
import { HttpHeaders } from '../rpc/client';
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
} from '../types';

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

  /**
   * Fetch CoinMetadata for a given coin type
   *
   * @param coinType fully qualified type names for the coin (e.g.,
   * 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
   *
   */
  abstract getCoinMetadata(coinType: string): Promise<CoinMetadata>;

  // Faucet
  /**
   * Request gas tokens from a faucet server
   * @param recipient the address for receiving the tokens
   * @param httpHeaders optional request headers
   */
  abstract requestSuiFromFaucet(
    recipient: SuiAddress,
    httpHeaders?: HttpHeaders
  ): Promise<FaucetResponse>;

  // Objects
  /**
   * Get all objects owned by an address
   */
  abstract getObjectsOwnedByAddress(
    addressOrObjectId: string
  ): Promise<SuiObjectInfo[]>;

  /**
   * Convenience method for getting all gas objects(SUI Tokens) owned by an address
   */
  abstract getGasObjectsOwnedByAddress(
    _address: string
  ): Promise<SuiObjectInfo[]>;

  /**
   * Convenience method for getting all coins objects owned by an address
   * @param typeArg optional argument for filter by coin type, e.g., '0x2::sui::SUI'
   */
  abstract getCoinBalancesOwnedByAddress(
    address: string,
    typeArg?: string
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
    exclude: ObjectId[]
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
    exclude: ObjectId[]
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
    end: GatewayTxSeqNumber
  ): Promise<GetTxnDigestsResponse>;

  /**
   * Get transactions for a given query criteria
   */
  abstract getTransactions(
    query: TransactionQuery,
    cursor: TransactionDigest | null,
    limit: number | null,
    order: Order
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
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string,
    requestType: ExecuteTransactionRequestType
  ): Promise<SuiExecuteTransactionResponse>;

  // Move info
  /**
   * Get Move function argument types like read, write and full access
   */
  abstract getMoveFunctionArgTypes(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveFunctionArgTypes>;

  /**
   * Get a map from module name to
   * structured representations of Move modules
   */
  abstract getNormalizedMoveModulesByPackage(
    objectId: string
  ): Promise<SuiMoveNormalizedModules>;

  /**
   * Get a structured representation of Move module
   */
  abstract getNormalizedMoveModule(
    objectId: string,
    moduleName: string
  ): Promise<SuiMoveNormalizedModule>;

  /**
   * Get a structured representation of Move function
   */
  abstract getNormalizedMoveFunction(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveNormalizedFunction>;

  /**
   * Get a structured representation of Move struct
   */
  abstract getNormalizedMoveStruct(
    objectId: string,
    moduleName: string,
    structName: string
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
    order: Order
  ): Promise<PaginatedEvents>;

  /**
   * Subscribe to get notifications whenever an event matching the filter occurs
   * @param filter - filter describing the subset of events to follow
   * @param onMessage - function to run when we receive a notification of a new event matching the filter
   */
  abstract subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId>;

  /**
   * Unsubscribe from an event subscription
   * @param id - subscription id to unsubscribe from (previously received from subscribeEvent)
   */
  abstract unsubscribeEvent(id: SubscriptionId): Promise<boolean>;
  // TODO: add more interface methods

  abstract dryRunTransaction(txBytes: string): Promise<TransactionEffects>;
}
