// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider } from './provider';
import { ErrorResponse, HttpHeaders, JsonRpcClient } from '../rpc/client';
import {
  Coin,
  ExecuteTransactionRequestType,
  GatewayTxSeqNumber,
  getObjectReference,
  GetTxnDigestsResponse,
  ObjectId,
  PaginatedTransactionDigests,
  SubscriptionId,
  SuiAddress,
  SuiEventEnvelope,
  SuiEventFilter,
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
  RpcApiVersion,
  parseVersionFromString,
  EventQuery,
  EventId,
  PaginatedEvents,
  FaucetResponse,
  Order,
  DevInspectResults,
  CoinMetadata,
  isValidTransactionDigest,
  isValidSuiAddress,
  isValidSuiObjectId,
  normalizeSuiAddress,
  normalizeSuiObjectId,
  CoinMetadataStruct,
  PaginatedCoins,
  SuiObjectResponse,
  GetOwnedObjectsResponse,
  DelegatedStake,
  ValidatorMetaData,
  SuiSystemState,
  CoinBalance,
  CoinSupply,
  CheckpointSummary,
  CheckpointContents,
  CheckpointDigest,
  Checkpoint,
  CheckPointContentsDigest,
  CommitteeInfo,
  DryRunTransactionResponse,
  SuiObjectDataOptions,
  SuiSystemStateSummary,
} from '../types';
import { DynamicFieldName, DynamicFieldPage } from '../types/dynamic_fields';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';
import { requestSuiFromFaucet } from '../rpc/faucet-client';
import { any, is, number, array } from 'superstruct';
import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import { LocalTxnDataSerializer } from '../signers/txn-data-serializers/local-txn-data-serializer';
import { toB64 } from '@mysten/bcs';
import { SerializedSignature } from '../cryptography/signature';
import { Connection, devnetConnection } from '../rpc/connection';
import { Transaction } from '../builder';

export const TARGETED_RPC_VERSION = '0.27.0';

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
   * TODO: Move to connection.
   */
  socketOptions?: WebsocketClientOptions;
  /**
   * Cache timeout in seconds for the RPC API Version
   */
  versionCacheTimeoutInSeconds?: number;

  /** Allow defining a custom RPC client to use */
  rpcClient?: JsonRpcClient;

  /** Allow defining a custom websocket client to use */
  websocketClient?: WebsocketClient;
};

const DEFAULT_OPTIONS: RpcProviderOptions = {
  skipDataValidation: true,
  socketOptions: DEFAULT_CLIENT_OPTIONS,
  versionCacheTimeoutInSeconds: 600,
};

export class JsonRpcProvider extends Provider {
  public connection: Connection;
  protected client: JsonRpcClient;
  protected wsClient: WebsocketClient;
  private rpcApiVersion: RpcApiVersion | undefined;
  private cacheExpiry: number | undefined;
  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param connection The `Connection` object containing configuration for the network.
   * @param options configuration options for the provider
   */
  constructor(
    // TODO: Probably remove the default endpoint here:
    connection: Connection = devnetConnection,
    public options: RpcProviderOptions = DEFAULT_OPTIONS,
  ) {
    super();

    this.connection = connection;

    const opts = { ...DEFAULT_OPTIONS, ...options };
    this.options = opts;
    // TODO: add header for websocket request
    this.client = opts.rpcClient ?? new JsonRpcClient(this.connection.fullnode);

    this.wsClient =
      opts.websocketClient ??
      new WebsocketClient(
        this.connection.websocket,
        opts.skipDataValidation!,
        opts.socketOptions,
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
        any(),
        this.options.skipDataValidation,
      );
      this.rpcApiVersion = parseVersionFromString(resp.info.version);
      this.cacheExpiry =
        // Date.now() is in milliseconds, but the timeout is in seconds
        Date.now() + (this.options.versionCacheTimeoutInSeconds ?? 0) * 1000;
      return this.rpcApiVersion;
    } catch (err) {
      console.warn('Error fetching version number of the RPC API', err);
    }
    return undefined;
  }

  async requestSuiFromFaucet(
    recipient: SuiAddress,
    httpHeaders?: HttpHeaders,
  ): Promise<FaucetResponse> {
    if (!this.connection.faucet) {
      throw new Error('Faucet URL is not specified');
    }
    return requestSuiFromFaucet(this.connection.faucet, recipient, httpHeaders);
  }

  // Coins
  async getCoins(
    owner: SuiAddress,
    coinType: string | null = null,
    cursor: ObjectId | null = null,
    limit: number | null = null,
  ): Promise<PaginatedCoins> {
    try {
      if (!owner || !isValidSuiAddress(normalizeSuiAddress(owner))) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getCoins',
        [owner, coinType, cursor, limit],
        PaginatedCoins,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error getting coins for owner ${owner}: ${err}`);
    }
  }

  async getAllCoins(
    owner: SuiAddress,
    cursor: ObjectId | null = null,
    limit: number | null = null,
  ): Promise<PaginatedCoins> {
    try {
      if (!owner || !isValidSuiAddress(normalizeSuiAddress(owner))) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getAllCoins',
        [owner, cursor, limit],
        PaginatedCoins,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error getting all coins for owner ${owner}: ${err}`);
    }
  }

  async getBalance(
    owner: SuiAddress,
    coinType: string | null = null,
  ): Promise<CoinBalance> {
    try {
      if (!owner || !isValidSuiAddress(normalizeSuiAddress(owner))) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getBalance',
        [owner, coinType],
        CoinBalance,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting balance for coin type ${coinType} for owner ${owner}: ${err}`,
      );
    }
  }

  async getAllBalances(owner: SuiAddress): Promise<CoinBalance[]> {
    try {
      if (!owner || !isValidSuiAddress(normalizeSuiAddress(owner))) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getAllBalances',
        [owner],
        array(CoinBalance),
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error getting all balances for owner ${owner}: ${err}`);
    }
  }

  async getCoinMetadata(coinType: string): Promise<CoinMetadata> {
    try {
      return await this.client.requestWithType(
        'sui_getCoinMetadata',
        [coinType],
        CoinMetadataStruct,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error fetching CoinMetadata for ${coinType}: ${err}`);
    }
  }

  async getTotalSupply(coinType: string): Promise<CoinSupply> {
    try {
      return await this.client.requestWithType(
        'sui_getTotalSupply',
        [coinType],
        CoinSupply,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching total supply for Coin type ${coinType}: ${err}`,
      );
    }
  }

  // RPC endpoint
  async call(endpoint: string, params: Array<any>): Promise<any> {
    try {
      const response = await this.client.request(endpoint, params);
      if (is(response, ErrorResponse)) {
        throw new Error(`RPC Error: ${response.error.message}`);
      }
      return response.result;
    } catch (err) {
      throw new Error(`Error calling RPC endpoint ${endpoint}: ${err}`);
    }
  }

  // Move info
  async getMoveFunctionArgTypes(
    packageId: string,
    moduleName: string,
    functionName: string,
  ): Promise<SuiMoveFunctionArgTypes> {
    try {
      return await this.client.requestWithType(
        'sui_getMoveFunctionArgTypes',
        [packageId, moduleName, functionName],
        SuiMoveFunctionArgTypes,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching Move function arg types with package object ID: ${packageId}, module name: ${moduleName}, function name: ${functionName}`,
      );
    }
  }

  async getNormalizedMoveModulesByPackage(
    packageId: string,
  ): Promise<SuiMoveNormalizedModules> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModulesByPackage',
        [packageId],
        SuiMoveNormalizedModules,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching package: ${err} for package ${packageId}`,
      );
    }
  }

  async getNormalizedMoveModule(
    packageId: string,
    moduleName: string,
  ): Promise<SuiMoveNormalizedModule> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModule',
        [packageId, moduleName],
        SuiMoveNormalizedModule,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching module: ${err} for package ${packageId}, module ${moduleName}`,
      );
    }
  }

  async getNormalizedMoveFunction(
    packageId: string,
    moduleName: string,
    functionName: string,
  ): Promise<SuiMoveNormalizedFunction> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveFunction',
        [packageId, moduleName, functionName],
        SuiMoveNormalizedFunction,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching function: ${err} for package ${packageId}, module ${moduleName} and function ${functionName}`,
      );
    }
  }

  async getNormalizedMoveStruct(
    packageId: string,
    moduleName: string,
    structName: string,
  ): Promise<SuiMoveNormalizedStruct> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveStruct',
        [packageId, moduleName, structName],
        SuiMoveNormalizedStruct,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching struct: ${err} for package ${packageId}, module ${moduleName} and struct ${structName}`,
      );
    }
  }

  // Objects
  async getObjectsOwnedByAddress(
    address: SuiAddress,
    typeFilter?: string,
  ): Promise<SuiObjectInfo[]> {
    try {
      if (!address || !isValidSuiAddress(normalizeSuiAddress(address))) {
        throw new Error('Invalid Sui address');
      }
      const objects = await this.client.requestWithType(
        'sui_getObjectsOwnedByAddress',
        [address],
        GetOwnedObjectsResponse,
        this.options.skipDataValidation,
      );
      // TODO: remove this once we migrated to the new queryObject API
      if (typeFilter) {
        return objects.filter(
          (obj: SuiObjectInfo) =>
            obj.type === typeFilter || obj.type.startsWith(typeFilter + '<'),
        );
      }
      return objects;
    } catch (err) {
      throw new Error(
        `Error fetching owned object: ${err} for address ${address}`,
      );
    }
  }

  async getGasObjectsOwnedByAddress(
    address: SuiAddress,
  ): Promise<SuiObjectInfo[]> {
    const objects = await this.getObjectsOwnedByAddress(address);
    return objects.filter((obj: SuiObjectInfo) => Coin.isSUI(obj));
  }

  async selectCoinsWithBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<SuiObjectResponse[]> {
    const coinsStruct = await this.getCoins(address, typeArg);
    const coinIds = coinsStruct.data.map((c) => c.coinObjectId);
    const coins = await this.getObjectBatch(coinIds, { showContent: true });
    return (await Coin.selectCoinsWithBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude,
    )) as SuiObjectResponse[];
  }

  async selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<SuiObjectResponse[]> {
    const coinsStruct = await this.getCoins(address, typeArg);
    const coinIds = coinsStruct.data.map((c) => c.coinObjectId);
    const coins = await this.getObjectBatch(coinIds, { showContent: true });
    return (await Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude,
    )) as SuiObjectResponse[];
  }

  async getObject(
    objectId: ObjectId,
    options?: SuiObjectDataOptions,
  ): Promise<SuiObjectResponse> {
    try {
      if (!objectId || !isValidSuiObjectId(normalizeSuiObjectId(objectId))) {
        throw new Error('Invalid Sui Object id');
      }
      return await this.client.requestWithType(
        'sui_getObject',
        [objectId, options],
        SuiObjectResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectId}`);
    }
  }

  async getObjectRef(objectId: ObjectId): Promise<SuiObjectRef | undefined> {
    const resp = await this.getObject(objectId);
    return getObjectReference(resp);
  }

  async getObjectBatch(
    objectIds: ObjectId[],
    options?: SuiObjectDataOptions,
  ): Promise<SuiObjectResponse[]> {
    try {
      const requests = objectIds.map((id) => {
        if (!id || !isValidSuiObjectId(normalizeSuiObjectId(id))) {
          throw new Error(`Invalid Sui Object id ${id}`);
        }
        return {
          method: 'sui_getObject',
          args: [id, options],
        };
      });
      return await this.client.batchRequestWithType(
        requests,
        SuiObjectResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching object info: ${err} for ids [${objectIds}]`,
      );
    }
  }

  // Transactions
  async getTransactions(
    query: TransactionQuery,
    cursor: TransactionDigest | null = null,
    limit: number | null = null,
    order: Order = 'descending',
  ): Promise<PaginatedTransactionDigests> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactions',
        [query, cursor, limit, order === 'descending'],
        PaginatedTransactionDigests,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting transactions for query: ${err} for query ${query}`,
      );
    }
  }

  async getTransactionsForObject(
    objectID: ObjectId,
    descendingOrder: boolean = true,
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ InputObject: objectID }, null, null, descendingOrder],
      },
      {
        method: 'sui_getTransactions',
        args: [{ MutatedObject: objectID }, null, null, descendingOrder],
      },
    ];

    try {
      if (!objectID || !isValidSuiObjectId(normalizeSuiObjectId(objectID))) {
        throw new Error('Invalid Sui Object id');
      }
      const results = await this.client.batchRequestWithType(
        requests,
        PaginatedTransactionDigests,
        this.options.skipDataValidation,
      );
      return [...results[0].data, ...results[1].data];
    } catch (err) {
      throw new Error(
        `Error getting transactions for object: ${err} for id ${objectID}`,
      );
    }
  }

  async getTransactionsForAddress(
    addressID: SuiAddress,
    descendingOrder: boolean = true,
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_getTransactions',
        args: [{ ToAddress: addressID }, null, null, descendingOrder],
      },
      {
        method: 'sui_getTransactions',
        args: [{ FromAddress: addressID }, null, null, descendingOrder],
      },
    ];
    try {
      if (!addressID || !isValidSuiAddress(normalizeSuiAddress(addressID))) {
        throw new Error('Invalid Sui address');
      }
      const results = await this.client.batchRequestWithType(
        requests,
        PaginatedTransactionDigests,
        this.options.skipDataValidation,
      );
      return [...results[0].data, ...results[1].data];
    } catch (err) {
      throw new Error(
        `Error getting transactions for address: ${err} for id ${addressID}`,
      );
    }
  }

  async getTransactionWithEffects(
    digest: TransactionDigest,
  ): Promise<SuiTransactionResponse> {
    try {
      if (!isValidTransactionDigest(digest)) {
        throw new Error('Invalid Transaction digest');
      }
      const resp = await this.client.requestWithType(
        'sui_getTransaction',
        [digest],
        SuiTransactionResponse,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting transaction with effects: ${err} for digest ${digest}`,
      );
    }
  }

  async getTransactionWithEffectsBatch(
    digests: TransactionDigest[],
  ): Promise<SuiTransactionResponse[]> {
    try {
      const requests = digests.map((d) => {
        if (!isValidTransactionDigest(d)) {
          throw new Error(`Invalid Transaction digest ${d}`);
        }
        return {
          method: 'sui_getTransaction',
          args: [d],
        };
      });
      return await this.client.batchRequestWithType(
        requests,
        SuiTransactionResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting transaction effects: ${err} for digests [${digests}]`,
      );
    }
  }

  async executeTransaction(
    txnBytes: Uint8Array | string,
    signature: SerializedSignature,
    requestType: ExecuteTransactionRequestType = 'WaitForEffectsCert',
  ): Promise<SuiTransactionResponse> {
    try {
      return await this.client.requestWithType(
        'sui_executeTransactionSerializedSig',
        [
          typeof txnBytes === 'string' ? txnBytes : toB64(txnBytes),
          signature,
          requestType,
        ],
        SuiTransactionResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error executing transaction with request type: ${err}`);
    }
  }

  async getTotalTransactionNumber(): Promise<number> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getTotalTransactionNumber',
        [],
        number(),
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error fetching total transaction number: ${err}`);
    }
  }

  async getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber,
  ): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactionsInRange',
        [start, end],
        GetTxnDigestsResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching transaction digests in range: ${err} for range ${start}-${end}`,
      );
    }
  }

  // Governance
  async getReferenceGasPrice(): Promise<number> {
    try {
      return await this.client.requestWithType(
        'sui_getReferenceGasPrice',
        [],
        number(),
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error getting the reference gas price ${err}`);
    }
  }

  async getDelegatedStakes(address: SuiAddress): Promise<DelegatedStake[]> {
    try {
      if (!address || !isValidSuiAddress(normalizeSuiAddress(address))) {
        throw new Error('Invalid Sui address');
      }
      const resp = await this.client.requestWithType(
        'sui_getDelegatedStakes',
        [address],
        array(DelegatedStake),
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error in getDelegatedStake: ${err}`);
    }
  }

  async getValidators(): Promise<ValidatorMetaData[]> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getValidators',
        [],
        array(ValidatorMetaData),
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error in getValidators: ${err}`);
    }
  }

  async getSuiSystemState(): Promise<SuiSystemState> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getSuiSystemState',
        [],
        SuiSystemState,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error in getSuiSystemState: ${err}`);
    }
  }

  async getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getLatestSuiSystemState',
        [],
        SuiSystemStateSummary,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error in getLatestSuiSystemState: ${err}`);
    }
  }

  // Events
  async getEvents(
    query: EventQuery,
    cursor: EventId | null,
    limit: number | null,
    order: Order = 'descending',
  ): Promise<PaginatedEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEvents',
        [query, cursor, limit, order === 'descending'],
        PaginatedEvents,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting events for query: ${err} for query ${query}`,
      );
    }
  }

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void,
  ): Promise<SubscriptionId> {
    return this.wsClient.subscribeEvent(filter, onMessage);
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    return this.wsClient.unsubscribeEvent(id);
  }

  async devInspectTransaction(
    sender: SuiAddress,
    tx: Transaction | UnserializedSignableTransaction | string | Uint8Array,
    gasPrice: number | null = null,
    epoch: number | null = null,
  ): Promise<DevInspectResults> {
    try {
      let devInspectTxBytes;
      if (Transaction.is(tx)) {
        devInspectTxBytes = await tx.build({ provider: this });
      } else if (typeof tx === 'string') {
        devInspectTxBytes = tx;
      } else if (tx instanceof Uint8Array) {
        devInspectTxBytes = toB64(tx);
      } else {
        devInspectTxBytes = toB64(
          await new LocalTxnDataSerializer(this).serializeToBytesWithoutGasInfo(
            sender,
            tx,
          ),
        );
      }

      const resp = await this.client.requestWithType(
        'sui_devInspectTransaction',
        [sender, devInspectTxBytes, gasPrice, epoch],
        DevInspectResults,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error dev inspect transaction with request type: ${err}`,
      );
    }
  }

  async dryRunTransaction(
    txBytes: Uint8Array,
  ): Promise<DryRunTransactionResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_dryRunTransaction',
        [toB64(txBytes)],
        DryRunTransactionResponse,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error dry running transaction with request type: ${err}`,
      );
    }
  }

  // Dynamic Fields
  async getDynamicFields(
    parent_object_id: ObjectId,
    cursor: ObjectId | null = null,
    limit: number | null = null,
  ): Promise<DynamicFieldPage> {
    try {
      if (
        !parent_object_id ||
        !isValidSuiObjectId(normalizeSuiObjectId(parent_object_id))
      ) {
        throw new Error('Invalid Sui Object id');
      }
      const resp = await this.client.requestWithType(
        'sui_getDynamicFields',
        [parent_object_id, cursor, limit],
        DynamicFieldPage,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting dynamic fields with request type: ${err} for parent_object_id: ${parent_object_id}, cursor: ${cursor} and limit: ${limit}.`,
      );
    }
  }

  async getDynamicFieldObject(
    parent_object_id: ObjectId,
    name: string | DynamicFieldName,
  ): Promise<SuiObjectResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getDynamicFieldObject',
        [parent_object_id, name],
        SuiObjectResponse,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting dynamic field object with request type: ${err} for parent_object_id: ${parent_object_id} and name: ${name}.`,
      );
    }
  }

  // Checkpoints
  async getLatestCheckpointSequenceNumber(): Promise<number> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getLatestCheckpointSequenceNumber',
        [],
        number(),
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error fetching latest checkpoint sequence number: ${err}`,
      );
    }
  }

  async getCheckpointSummary(
    sequence_number: number,
  ): Promise<CheckpointSummary> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpointSummary',
        [sequence_number],
        CheckpointSummary,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint summary with request type: ${err} for sequence number: ${sequence_number}.`,
      );
    }
  }

  async getCheckpointSummaryByDigest(
    digest: CheckpointDigest,
  ): Promise<CheckpointSummary> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpointSummaryByDigest',
        [digest],
        CheckpointSummary,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint summary with request type: ${err} for digest: ${digest}.`,
      );
    }
  }

  async getCheckpointContents(
    sequence_number: number | CheckPointContentsDigest,
  ): Promise<CheckpointContents> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpointContents',
        [sequence_number],
        CheckpointContents,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint contents with request type: ${err} for sequence number: ${sequence_number}.`,
      );
    }
  }

  async getCheckpointContentsByDigest(
    digest: CheckPointContentsDigest,
  ): Promise<CheckpointContents> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpointContentsByDigest',
        [digest],
        CheckpointContents,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint summary with request type: ${err} for digest: ${digest}.`,
      );
    }
  }

  async getCheckpoint(id: CheckpointDigest | number): Promise<Checkpoint> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpoint',
        [id],
        Checkpoint,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint with request type: ${err} for id: ${id}.`,
      );
    }
  }

  async getCommitteeInfo(epoch?: number): Promise<CommitteeInfo> {
    try {
      const committeeInfo = await this.client.requestWithType(
        'sui_getCommitteeInfo',
        [epoch],
        CommitteeInfo,
      );

      return committeeInfo;
    } catch (error) {
      throw new Error(`Error getCommitteeInfo : ${error}`);
    }
  }
}
