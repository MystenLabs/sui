// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { HttpHeaders } from '../rpc/client';
import {
  TransactionDigest,
  GetTxnDigestsResponse,
  GatewayTxSeqNumber,
  SuiObjectInfo,
  SuiObjectResponse,
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
  SuiAddress,
  ObjectId,
  TransactionQuery,
  PaginatedTransactionDigests,
  EventQuery,
  PaginatedEvents,
  EventId,
  RpcApiVersion,
  FaucetResponse,
  Order,
  CoinMetadata,
  DevInspectResults,
  DelegatedStake,
  PaginatedCoins,
  CoinBalance,
  CoinSupply,
  CheckpointDigest,
  CommitteeInfo,
  Checkpoint,
  DryRunTransactionResponse,
  SuiTransactionResponse,
  SuiObjectDataOptions,
  SuiSystemStateSummary,
  CoinStruct,
} from '../types';
import { Provider } from './provider';

import { DynamicFieldName, DynamicFieldPage } from '../types/dynamic_fields';
import { SerializedSignature } from '../cryptography/signature';
import { Transaction } from '../builder';

export class VoidProvider extends Provider {
  // API Version
  async getRpcApiVersion(): Promise<RpcApiVersion | undefined> {
    throw this.newError('getRpcApiVersion');
  }

  // Governance
  async getReferenceGasPrice(): Promise<number> {
    throw this.newError('getReferenceGasPrice');
  }

  async getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
    throw this.newError('getLatestSuiSystemState');
  }

  async getDelegatedStakes(_address: SuiAddress): Promise<DelegatedStake[]> {
    throw this.newError('getDelegatedStakes');
  }

  // Faucet
  async requestSuiFromFaucet(
    _recipient: SuiAddress,
    _httpHeaders?: HttpHeaders,
  ): Promise<FaucetResponse> {
    throw this.newError('requestSuiFromFaucet');
  }

  // RPC Endpoint
  call(_endpoint: string, _params: any[]): Promise<any> {
    throw this.newError('call');
  }

  // Coins
  async getCoins(): Promise<PaginatedCoins> {
    throw this.newError('getCoins');
  }

  async getAllCoins(): Promise<PaginatedCoins> {
    throw this.newError('getAllCoins');
  }

  async getBalance(): Promise<CoinBalance> {
    throw this.newError('getBalance');
  }

  async getAllBalances(): Promise<CoinBalance[]> {
    throw this.newError('getAllBalances');
  }

  async getCoinMetadata(): Promise<CoinMetadata> {
    throw new Error('getCoinMetadata');
  }

  async getTotalSupply(): Promise<CoinSupply> {
    throw new Error('getTotalSupply');
  }

  // Objects
  async getObjectsOwnedByAddress(
    _address: string,
    _typefilter?: string,
  ): Promise<SuiObjectInfo[]> {
    throw this.newError('getObjectsOwnedByAddress');
  }

  async selectCoinsWithBalanceGreaterThanOrEqual(
    _address: string,
    _amount: bigint,
    _typeArg: string,
    _exclude: ObjectId[] = [],
  ): Promise<CoinStruct[]> {
    throw this.newError('selectCoinsWithBalanceGreaterThanOrEqual');
  }

  async selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    _address: string,
    _amount: bigint,
    _typeArg: string,
    _exclude: ObjectId[],
  ): Promise<CoinStruct[]> {
    throw this.newError('selectCoinSetWithCombinedBalanceGreaterThanOrEqual');
  }

  async getObject(_objectId: string): Promise<SuiObjectResponse> {
    throw this.newError('getObject');
  }

  async getObjectRef(_objectId: string): Promise<SuiObjectRef | undefined> {
    throw this.newError('getObjectRef');
  }

  async getObjectBatch(
    _objectIds: ObjectId[],
    _options?: SuiObjectDataOptions,
  ): Promise<SuiObjectResponse[]> {
    throw this.newError('getObjectBatch');
  }

  // Transactions
  async getTransaction(
    _digest: TransactionDigest,
  ): Promise<SuiTransactionResponse> {
    throw this.newError('getTransaction');
  }

  async executeTransaction(
    _txnBytes: Uint8Array,
    _signature: SerializedSignature | SerializedSignature[],
    _requestType: ExecuteTransactionRequestType,
  ): Promise<SuiTransactionResponse> {
    throw this.newError('executeTransaction with request Type');
  }

  devInspectTransaction(
    _sender: SuiAddress,
    _txn: Transaction | string | Uint8Array,
    _gasPrice: number | null = null,
    _epoch: number | null = null,
  ): Promise<DevInspectResults> {
    throw this.newError('devInspectTransaction');
  }

  dryRunTransaction(_txBytes: Uint8Array): Promise<DryRunTransactionResponse> {
    throw this.newError('dryRunTransaction');
  }

  getDynamicFields(
    _parent_object_id: ObjectId,
    _cursor: ObjectId | null = null,
    _limit: number | null = null,
  ): Promise<DynamicFieldPage> {
    throw this.newError('getDynamicFields');
  }

  getDynamicFieldObject(
    _parent_object_id: ObjectId,
    _name: string | DynamicFieldName,
  ): Promise<SuiObjectResponse> {
    throw this.newError('getDynamicFieldObject');
  }

  async getTotalTransactionNumber(): Promise<number> {
    throw this.newError('getTotalTransactionNumber');
  }

  async getTransactionDigestsInRange(
    _start: GatewayTxSeqNumber,
    _end: GatewayTxSeqNumber,
  ): Promise<GetTxnDigestsResponse> {
    throw this.newError('getTransactionDigestsInRange');
  }

  async getMoveFunctionArgTypes(
    _objectId: string,
    _moduleName: string,
    _functionName: string,
  ): Promise<SuiMoveFunctionArgTypes> {
    throw this.newError('getMoveFunctionArgTypes');
  }

  async getNormalizedMoveModulesByPackage(
    _objectId: string,
  ): Promise<SuiMoveNormalizedModules> {
    throw this.newError('getNormalizedMoveModulesByPackage');
  }

  async getNormalizedMoveModule(
    _objectId: string,
    _moduleName: string,
  ): Promise<SuiMoveNormalizedModule> {
    throw this.newError('getNormalizedMoveModule');
  }

  async getNormalizedMoveFunction(
    _objectId: string,
    _moduleName: string,
    _functionName: string,
  ): Promise<SuiMoveNormalizedFunction> {
    throw this.newError('getNormalizedMoveFunction');
  }

  async getNormalizedMoveStruct(
    _objectId: string,
    _oduleName: string,
    _structName: string,
  ): Promise<SuiMoveNormalizedStruct> {
    throw this.newError('getNormalizedMoveStruct');
  }

  async syncAccountState(_address: string): Promise<any> {
    throw this.newError('syncAccountState');
  }

  async subscribeEvent(
    _filter: SuiEventFilter,
    _onMessage: (event: SuiEventEnvelope) => void,
  ): Promise<SubscriptionId> {
    throw this.newError('subscribeEvent');
  }

  async unsubscribeEvent(_id: SubscriptionId): Promise<boolean> {
    throw this.newError('unsubscribeEvent');
  }

  private newError(operation: string): Error {
    return new Error(`Please use a valid provider for ${operation}`);
  }

  async getTransactions(
    _query: TransactionQuery,
    _cursor: TransactionDigest | null,
    _limit: number | null,
    _order: Order,
  ): Promise<PaginatedTransactionDigests> {
    throw this.newError('getTransactions');
  }

  async getEvents(
    _query: EventQuery,
    _cursor: EventId | null,
    _limit: number | null,
    _order: Order,
  ): Promise<PaginatedEvents> {
    throw this.newError('getEvents');
  }

  // Checkpoints
  async getLatestCheckpointSequenceNumber(): Promise<number> {
    throw this.newError('getLatestCheckpointSequenceNumber');
  }

  async getCheckpoint(_id: CheckpointDigest | number): Promise<Checkpoint> {
    throw this.newError('getCheckpoint');
  }

  async getCommitteeInfo(_epoch?: number): Promise<CommitteeInfo> {
    throw this.newError('getCommitteeInfo');
  }
}
