// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignatureScheme } from '../cryptography/publickey';
import {
  CoinDenominationInfoResponse,
  GetObjectDataResponse,
  SuiObjectInfo,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  SuiTransactionResponse,
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
  ObjectOwner,
  SuiEvents,
  PaginatedTransactionDigests,
  TransactionQuery,
  Ordering,
  RpcApiVersion,
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
   * Method to look up denomination of a specific type of coin.
   * TODO: now only SUI coins are supported, will scale to other types
   * based on their definitions in Move.
   *
   * @param coin_type coin type, e.g., '0x2::sui::SUI'
   * @return denomination info of the coin including,
   * coin type, the same as input coin type
   * basic unit, the min unit of the coin, e.g., MIST;
   * canonical unit, the commonly used unit, e.g., SUI;
   * denomination, the value of 1 canonical over 1 basic unit,
   * for example 1_000_000_000 = 1 SUI / 1 MIST;
   * decimal number, the number of zeros in the denomination,
   * e.g., 9 here for SUI coin.
   */
  abstract getCoinDenominationInfo(
    coin_type: string
  ): CoinDenominationInfoResponse;

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
    order: Ordering
  ): Promise<PaginatedTransactionDigests>;

  /**
   * Get total number of transactions
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTotalTransactionNumber(): Promise<number>;

  abstract executeTransaction(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string
  ): Promise<SuiTransactionResponse>;

  /**
   * This is under development endpoint on Fullnode that will eventually
   * replace the other `executeTransaction` that's only available on the
   * Gateway
   */
  abstract executeTransactionWithRequestType(
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

  abstract syncAccountState(address: string): Promise<any>;

  /**
   * Get events for one transaction
   * @param digest transaction digest to search by
   * @param count max result count
   */
  abstract getEventsByTransaction(
    digest: TransactionDigest,
    count: number
  ): Promise<SuiEvents>;

  /**
   * Get events emitted from within the specified Move module
   * @param package_ Move package object ID
   * @param module Move module name
   * @param count max result count
   * @param startTime start of time range
   * @param endTime end of time range, exclusive
   */
  abstract getEventsByModule(
    packageId: ObjectId,
    module: string,
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

  /**
   * Get events with a matching Move type name
   * @param moveEventStructName Move struct type name
   * @param count max result count
   * @param startTime start of time range to search
   * @param endTime end of time range
   */
  abstract getEventsByMoveEventStructName(
    moveEventStructName: string,
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

  /**
   * Get events from transactions sent by a specific address
   * @param sender Sui address of the sender of the transaction that generated the event
   * @param count max result count
   * @param startTime start of time range to search
   * @param endTime end of time range
   */
  abstract getEventsBySender(
    sender: SuiAddress,
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

  /**
   * Get events with a matching recipient
   * @param recipient object owner that received the transaction that generated the event
   * @param count max result count
   * @param startTime start of time range to search
   * @param endTime end of time range
   */
  abstract getEventsByRecipient(
    recipient: ObjectOwner,
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

  /**
   * Get events involving the given object
   * @param object object id created, mutated, or deleted in events
   * @param count max result count
   * @param startTime start of time range to search
   * @param endTime end of time range
   */
  abstract getEventsByObject(
    object: ObjectId,
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

  /**
   * Get all events within the given time span
   * @param count max result count
   * @param startTime start of time range to search
   * @param endTime end of time range
   */
  abstract getEventsByTimeRange(
    count: number,
    startTime: number,
    endTime: number
  ): Promise<SuiEvents>;

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
}
