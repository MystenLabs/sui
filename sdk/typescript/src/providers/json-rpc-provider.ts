// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider } from './provider';
import { JsonRpcClient } from '../rpc/client';
import {
  isGetObjectDataResponse,
  isGetOwnedObjectsResponse,
  isGetTxnDigestsResponse,
  isGetTxnDigestsResponse__DEPRECATED,
  isPaginatedTransactionDigests,
  isSuiEvents,
  isSuiExecuteTransactionResponse,
  isSuiMoveFunctionArgTypes,
  isSuiMoveNormalizedFunction,
  isSuiMoveNormalizedModule,
  isSuiMoveNormalizedModules,
  isSuiMoveNormalizedStruct,
  isSuiTransactionResponse,
} from '../types/index.guard';
import {
  Coin,
  DEFAULT_END_TIME,
  DEFAULT_START_TIME,
  EVENT_QUERY_MAX_LIMIT,
  ExecuteTransactionRequestType,
  GatewayTxSeqNumber,
  GetObjectDataResponse,
  getObjectReference,
  GetTxnDigestsResponse,
  ObjectId,
  ObjectOwner,
  Ordering,
  PaginatedTransactionDigests,
  SubscriptionId,
  SuiAddress,
  SuiEventEnvelope,
  SuiEventFilter,
  SuiEvents,
  SuiExecuteTransactionResponse,
  SuiMoveFunctionArgTypes,
  SuiMoveNormalizedFunction,
  SuiMoveNormalizedModule,
  SuiMoveNormalizedModules,
  SuiMoveNormalizedStruct,
  SuiObjectInfo,
  SuiObjectRef,
  SuiTransactionResponse,
  TransactionDigest,
  TransactionQuery,
  SUI_TYPE_ARG,
} from '../types';
import { SignatureScheme } from '../cryptography/publickey';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';

const isNumber = (val: any): val is number => typeof val === 'number';
const isAny = (_val: any): _val is any => true;

const PRE_PAGINATION_API_VERSION = '0.11.0';
export const LATEST_RPC_API_VERSION = 'latest';

export class JsonRpcProvider extends Provider {
  protected client: JsonRpcClient;
  protected wsClient: WebsocketClient;
  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param endpoint URL to the Sui RPC endpoint
   * @param skipDataValidation default to `true`. If set to `false`, the rpc
   * client will throw an error if the responses from the RPC server do not
   * conform to the schema defined in the TypeScript SDK. If set to `true`, the
   * rpc client will log the mismatch as a warning message instead of throwing an
   * error. The mismatches often happen when the SDK is in a different version than
   * the RPC server. Skipping the validation can maximize
   * the version compatibility of the SDK, as not all the schema
   * changes in the RPC response will affect the caller, but the caller needs to
   * understand that the data may not match the TypeSrcript definitions.
   * @param rpcAPIVersion controls which type of RPC API version to use.
   */
  constructor(
    public endpoint: string,
    public skipDataValidation: boolean = true,
    // TODO: Update the default value after we deploy 0.12.0
    private rpcAPIVersion: string = PRE_PAGINATION_API_VERSION,
    public socketOptions: WebsocketClientOptions = DEFAULT_CLIENT_OPTIONS
  ) {
    super();

    this.client = new JsonRpcClient(endpoint);
    this.wsClient = new WebsocketClient(
      endpoint,
      skipDataValidation,
      socketOptions
    );
  }

  // Move info
  async getMoveFunctionArgTypes(
    packageId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveFunctionArgTypes> {
    try {
      return await this.client.requestWithType(
        'sui_getMoveFunctionArgTypes',
        [packageId, moduleName, functionName],
        isSuiMoveFunctionArgTypes,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching Move function arg types with package object ID: ${packageId}, module name: ${moduleName}, function name: ${functionName}`
      );
    }
  }

  async getNormalizedMoveModulesByPackage(
    packageId: string
  ): Promise<SuiMoveNormalizedModules> {
    // TODO: Add caching since package object does not change
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModulesByPackage',
        [packageId],
        isSuiMoveNormalizedModules,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching package: ${err} for package ${packageId}`
      );
    }
  }

  async getNormalizedMoveModule(
    packageId: string,
    moduleName: string
  ): Promise<SuiMoveNormalizedModule> {
    // TODO: Add caching since package object does not change
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModule',
        [packageId, moduleName],
        isSuiMoveNormalizedModule,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching module: ${err} for package ${packageId}, module ${moduleName}}`
      );
    }
  }

  async getNormalizedMoveFunction(
    packageId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveNormalizedFunction> {
    // TODO: Add caching since package object does not change
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveFunction',
        [packageId, moduleName, functionName],
        isSuiMoveNormalizedFunction,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching function: ${err} for package ${packageId}, module ${moduleName} and function ${functionName}}`
      );
    }
  }

  async getNormalizedMoveStruct(
    packageId: string,
    moduleName: string,
    structName: string
  ): Promise<SuiMoveNormalizedStruct> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveStruct',
        [packageId, moduleName, structName],
        isSuiMoveNormalizedStruct,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching struct: ${err} for package ${packageId}, module ${moduleName} and struct ${structName}}`
      );
    }
  }

  // Objects
  async getObjectsOwnedByAddress(address: string): Promise<SuiObjectInfo[]> {
    try {
      return await this.client.requestWithType(
        'sui_getObjectsOwnedByAddress',
        [address],
        isGetOwnedObjectsResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching owned object: ${err} for address ${address}`
      );
    }
  }

  async getGasObjectsOwnedByAddress(address: string): Promise<SuiObjectInfo[]> {
    const objects = await this.getObjectsOwnedByAddress(address);
    return objects.filter((obj: SuiObjectInfo) => Coin.isSUI(obj));
  }

  async getCoinBalancesOwnedByAddress(
    address: string,
    typeArg?: string
  ): Promise<GetObjectDataResponse[]> {
    const objects = await this.getObjectsOwnedByAddress(address);
    const coinIds = objects
      .filter(
        (obj: SuiObjectInfo) =>
          Coin.isCoin(obj) &&
          (typeArg === undefined || typeArg === Coin.getCoinTypeArg(obj))
      )
      .map((c) => c.objectId);

    return await this.getObjectBatch(coinIds);
  }

  async selectCoinsWithBalanceGreaterThanOrEqual(
    address: string,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = []
  ): Promise<GetObjectDataResponse[]> {
    const coins = await this.getCoinBalancesOwnedByAddress(address, typeArg);
    return (await Coin.selectCoinsWithBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude
    )) as GetObjectDataResponse[];
  }

  async selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    address: string,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = []
  ): Promise<GetObjectDataResponse[]> {
    const coins = await this.getCoinBalancesOwnedByAddress(address, typeArg);
    return (await Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude
    )) as GetObjectDataResponse[];
  }

  async getObjectsOwnedByObject(objectId: string): Promise<SuiObjectInfo[]> {
    try {
      return await this.client.requestWithType(
        'sui_getObjectsOwnedByObject',
        [objectId],
        isGetOwnedObjectsResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching owned object: ${err} for objectId ${objectId}`
      );
    }
  }

  async getObject(objectId: string): Promise<GetObjectDataResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getObject',
        [objectId],
        isGetObjectDataResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectId}`);
    }
  }

  async getObjectRef(objectId: string): Promise<SuiObjectRef | undefined> {
    const resp = await this.getObject(objectId);
    return getObjectReference(resp);
  }

  async getObjectBatch(objectIds: string[]): Promise<GetObjectDataResponse[]> {
    const requests = objectIds.map((id) => ({
      method: 'sui_getObject',
      args: [id],
    }));
    try {
      return await this.client.batchRequestWithType(
        requests,
        isGetObjectDataResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectIds}`);
    }
  }

  // Transactions
  async getTransactions(
    query: TransactionQuery,
    cursor: TransactionDigest | null,
    limit: number | null,
    order: Ordering
  ): Promise<PaginatedTransactionDigests> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactions',
        [query, cursor, limit, order],
        isPaginatedTransactionDigests,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting transactions for query: ${err} for query ${query}`
      );
    }
  }

  async getTransactionsForObject(
    objectID: string
  ): Promise<GetTxnDigestsResponse> {
    // TODO: remove after we deploy 0.12.0 DevNet
    if (this.rpcAPIVersion === PRE_PAGINATION_API_VERSION) {
      const requests = [
        {
          method: 'sui_getTransactionsByInputObject',
          args: [objectID],
        },
        {
          method: 'sui_getTransactionsByMutatedObject',
          args: [objectID],
        },
      ];

      try {
        const results = await this.client.batchRequestWithType(
          requests,
          isGetTxnDigestsResponse__DEPRECATED,
          this.skipDataValidation
        );
        return [...results[0], ...results[1]].map((tx) => tx[1]);
      } catch (err) {
        throw new Error(
          `Error getting transactions for object: ${err} for id ${objectID}`
        );
      }
    }
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ InputObject: objectID }, null, null, 'Ascending'],
      },
      {
        method: 'sui_getTransactions',
        args: [{ MutatedObject: objectID }, null, null, 'Ascending'],
      },
    ];

    try {
      const results = await this.client.batchRequestWithType(
        requests,
        isPaginatedTransactionDigests,
        this.skipDataValidation
      );
      return [...results[0].data, ...results[1].data];
    } catch (err) {
      throw new Error(
        `Error getting transactions for object: ${err} for id ${objectID}`
      );
    }
  }

  async getTransactionsForAddress(
    addressID: string
  ): Promise<GetTxnDigestsResponse> {
    // TODO: remove after we deploy 0.12.0 DevNet
    if (this.rpcAPIVersion === PRE_PAGINATION_API_VERSION) {
      const requests = [
        {
          method: 'sui_getTransactionsToAddress',
          args: [addressID],
        },
        {
          method: 'sui_getTransactionsFromAddress',
          args: [addressID],
        },
      ];
      try {
        const results = await this.client.batchRequestWithType(
          requests,
          isGetTxnDigestsResponse__DEPRECATED,
          this.skipDataValidation
        );
        return [...results[0], ...results[1]].map((r) => r[1]);
      } catch (err) {
        throw new Error(
          `Error getting transactions for address: ${err} for id ${addressID}`
        );
      }
    }
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ ToAddress: addressID }, null, null, 'Ascending'],
      },
      {
        method: 'sui_getTransactions',
        args: [{ FromAddress: addressID }, null, null, 'Ascending'],
      },
    ];
    try {
      const results = await this.client.batchRequestWithType(
        requests,
        isPaginatedTransactionDigests,
        this.skipDataValidation
      );
      return [...results[0].data, ...results[1].data];
    } catch (err) {
      throw new Error(
        `Error getting transactions for address: ${err} for id ${addressID}`
      );
    }
  }

  async getTransactionWithEffects(
    digest: TransactionDigest
  ): Promise<SuiTransactionResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getTransaction',
        [digest],
        isSuiTransactionResponse,
        this.skipDataValidation
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting transaction with effects: ${err} for digest ${digest}`
      );
    }
  }

  async getTransactionWithEffectsBatch(
    digests: TransactionDigest[]
  ): Promise<SuiTransactionResponse[]> {
    const requests = digests.map((d) => ({
      method: 'sui_getTransaction',
      args: [d],
    }));
    try {
      return await this.client.batchRequestWithType(
        requests,
        isSuiTransactionResponse,
        this.skipDataValidation
      );
    } catch (err) {
      const list = digests.join(', ').substring(0, -2);
      throw new Error(
        `Error getting transaction effects: ${err} for digests [${list}]`
      );
    }
  }

  async executeTransaction(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string
  ): Promise<SuiTransactionResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_executeTransaction',
        [txnBytes, signatureScheme, signature, pubkey],
        isSuiTransactionResponse,
        this.skipDataValidation
      );
      return resp;
    } catch (err) {
      throw new Error(`Error executing transaction: ${err}}`);
    }
  }

  async executeTransactionWithRequestType(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string,
    requestType: ExecuteTransactionRequestType = 'WaitForEffectsCert'
  ): Promise<SuiExecuteTransactionResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_executeTransaction',
        [txnBytes, signatureScheme, signature, pubkey, requestType],
        isSuiExecuteTransactionResponse,
        this.skipDataValidation
      );
      return resp;
    } catch (err) {
      throw new Error(`Error executing transaction with request type: ${err}}`);
    }
  }

  async getTotalTransactionNumber(): Promise<number> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getTotalTransactionNumber',
        [],
        isNumber,
        this.skipDataValidation
      );
      return resp;
    } catch (err) {
      throw new Error(`Error fetching total transaction number: ${err}`);
    }
  }

  async getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber
  ): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactionsInRange',
        [start, end],
        isGetTxnDigestsResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching transaction digests in range: ${err} for range ${start}-${end}`
      );
    }
  }

  async getRecentTransactions(count: number): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getRecentTransactions',
        [count],
        isGetTxnDigestsResponse,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching recent transactions: ${err} for count ${count}`
      );
    }
  }

  async syncAccountState(address: string): Promise<any> {
    try {
      return await this.client.requestWithType(
        'sui_syncAccountState',
        [address],
        isAny,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error sync account address for address: ${address} with error: ${err}`
      );
    }
  }

  // Events

  async getEventsByTransaction(
    digest: TransactionDigest,
    count: number = EVENT_QUERY_MAX_LIMIT
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByTransaction',
        [digest, count],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by transaction: ${digest}, with error: ${err}`
      );
    }
  }

  async getEventsByModule(
    package_: string,
    module: string,
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByModule',
        [package_, module, count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by transaction module: ${package_}::${module}, with error: ${err}`
      );
    }
  }

  async getEventsByMoveEventStructName(
    moveEventStructName: string,
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByMoveEventStructName',
        [moveEventStructName, count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by move event struct name: ${moveEventStructName}, with error: ${err}`
      );
    }
  }

  async getEventsBySender(
    sender: SuiAddress,
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsBySender',
        [sender, count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by sender: ${sender}, with error: ${err}`
      );
    }
  }

  async getEventsByRecipient(
    recipient: ObjectOwner,
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByRecipient',
        [recipient, count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by receipient: ${recipient}, with error: ${err}`
      );
    }
  }

  async getEventsByObject(
    object: ObjectId,
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByObject',
        [object, count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by object: ${object}, with error: ${err}`
      );
    }
  }

  async getEventsByTimeRange(
    count: number = EVENT_QUERY_MAX_LIMIT,
    startTime: number = DEFAULT_START_TIME,
    endTime: number = DEFAULT_END_TIME
  ): Promise<SuiEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEventsByTimeRange',
        [count, startTime, endTime],
        isSuiEvents,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting events by time range: ${startTime} thru ${endTime}, with error: ${err}`
      );
    }
  }

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId> {
    return this.wsClient.subscribeEvent(filter, onMessage);
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    return this.wsClient.unsubscribeEvent(id);
  }
}
