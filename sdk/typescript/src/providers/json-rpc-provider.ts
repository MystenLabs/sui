// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider } from './provider';
import { JsonRpcClient } from '../rpc/client';
import {
  isGetObjectDataResponse,
  isGetOwnedObjectsResponse,
  isGetTxnDigestsResponse,
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
  CoinDenominationInfoResponse,
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
  normalizeSuiAddress,
  RpcApiVersion,
  parseVersionFromString,
} from '../types';
import { SignatureScheme } from '../cryptography/publickey';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';

const isNumber = (val: any): val is number => typeof val === 'number';
const isAny = (_val: any): _val is any => true;

/**
 * Configuration options for the JsonRpcProvider. If the value of a field is not provided,
 * value in `DEFAULT_OPTIONS` for that field will be used
 */
export type RpcProviderOptions = {
  /**
   * Default to `true`. If set to `false`, the rpc
   * client will throw an error if the responses from the RPC server do not
   * conform to the schema defined in the TypeScript SDK. If set to `true`, the
   * rpc client will log the mismatch as a warning message instead of throwing an
   * error. The mismatches often happen when the SDK is in a different version than
   * the RPC server. Skipping the validation can maximize
   * the version compatibility of the SDK, as not all the schema
   * changes in the RPC response will affect the caller, but the caller needs to
   * understand that the data may not match the TypeSrcript definitions.
   */
  skipDataValidation?: boolean;
  /**
   * Configuration options for the websocket connection
   */
  socketOptions?: WebsocketClientOptions;
  /**
   * Cache timeout in seconds for the RPC API Version
   */
  versionCacheTimoutInSeconds?: number;
};

const DEFAULT_OPTIONS: RpcProviderOptions = {
  skipDataValidation: true,
  socketOptions: DEFAULT_CLIENT_OPTIONS,
  versionCacheTimoutInSeconds: 600,
};

export class JsonRpcProvider extends Provider {
  protected client: JsonRpcClient;
  protected wsClient: WebsocketClient;
  private rpcApiVersion: RpcApiVersion | undefined;
  private cacheExpiry: number | undefined;
  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param endpoint URL to the Sui RPC endpoint
   * @param options configuration options for the provider
   */
  constructor(
    public endpoint: string,
    public options: RpcProviderOptions = DEFAULT_OPTIONS
  ) {
    super();

    const opts = { ...DEFAULT_OPTIONS, ...options };

    this.client = new JsonRpcClient(endpoint);
    this.wsClient = new WebsocketClient(
      endpoint,
      opts.skipDataValidation!,
      opts.socketOptions
    );
  }

  async getRpcApiVersion(): Promise<RpcApiVersion | undefined> {
    if (
      this.rpcApiVersion &&
      this.cacheExpiry &&
      this.cacheExpiry <= Date.now()
    ) {
      return this.rpcApiVersion;
    }
    try {
      const resp = await this.client.requestWithType(
        'rpc.discover',
        [],
        isAny,
        this.options.skipDataValidation
      );
      this.rpcApiVersion = parseVersionFromString(resp.info.version);
      this.cacheExpiry =
        Date.now() + (this.options.versionCacheTimoutInSeconds ?? 0);
      return this.rpcApiVersion;
    } catch (err) {
      console.warn('Error fetching version number of the RPC API', err);
    }
    return undefined;
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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

  getCoinDenominationInfo(coinType: string): CoinDenominationInfoResponse {
    const [packageId, module, symbol] = coinType.split('::');
    if (
      normalizeSuiAddress(packageId) !== normalizeSuiAddress('0x2') ||
      module != 'sui' ||
      symbol !== 'SUI'
    ) {
      throw new Error(
        'only SUI coin is supported in getCoinDenominationInfo for now.'
      );
    }

    return {
      coinType: coinType,
      basicUnit: 'MIST',
      decimalNumber: 9,
    };
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error getting transactions for query: ${err} for query ${query}`
      );
    }
  }

  async getTransactionsForObject(
    objectID: string,
    ordering: Ordering = 'Descending'
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ InputObject: objectID }, null, null, ordering],
      },
      {
        method: 'sui_getTransactions',
        args: [{ MutatedObject: objectID }, null, null, ordering],
      },
    ];

    try {
      const results = await this.client.batchRequestWithType(
        requests,
        isPaginatedTransactionDigests,
        this.options.skipDataValidation
      );
      return [...results[0].data, ...results[1].data];
    } catch (err) {
      throw new Error(
        `Error getting transactions for object: ${err} for id ${objectID}`
      );
    }
  }

  async getTransactionsForAddress(
    addressID: string,
    ordering: Ordering = 'Descending'
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ ToAddress: addressID }, null, null, ordering],
      },
      {
        method: 'sui_getTransactions',
        args: [{ FromAddress: addressID }, null, null, ordering],
      },
    ];
    try {
      const results = await this.client.batchRequestWithType(
        requests,
        isPaginatedTransactionDigests,
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
        this.options.skipDataValidation
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
