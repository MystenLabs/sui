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
  RpcApiVersion,
  parseVersionFromString,
  EventQuery,
  EventId,
  PaginatedEvents,
  FaucetResponse,
  Order,
  TransactionEffects,
  DevInspectResults,
  CoinMetadata,
  versionToString,
  isValidTransactionDigest,
  isValidSuiAddress,
  isValidSuiObjectId,
  normalizeSuiAddress,
  normalizeSuiObjectId,
  SuiTransactionAuthSignersResponse,
  CoinMetadataStruct,
  PaginatedCoins,
  GetObjectDataResponse,
  GetOwnedObjectsResponse,
  DelegatedStake,
  ValidatorMetaData,
  SuiSystemState,
  CoinBalance,
  CoinSupply,
  CheckpointSummary,
  CheckpointContents,
  CheckpointDigest,
  CheckPointContentsDigest,
  CommitteeInfo,
} from '../types';
import { DynamicFieldPage } from '../types/dynamic_fields';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';
import { ApiEndpoints, Network, NETWORK_TO_API } from '../utils/api-endpoints';
import { requestSuiFromFaucet } from '../rpc/faucet-client';
import { lt } from '@suchipi/femver';
import { any, is, number, array } from 'superstruct';
import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import { LocalTxnDataSerializer } from '../signers/txn-data-serializers/local-txn-data-serializer';
import { toB64 } from '@mysten/bcs';
import { SerializedSignature } from '../cryptography/signature';

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
  /**
   * URL to a faucet(optional). If you initialize `JsonRpcProvider`
   * with a known `Network` value, this will be populated with a default
   * value
   */
  faucetURL?: string;
};

const DEFAULT_OPTIONS: RpcProviderOptions = {
  skipDataValidation: true,
  socketOptions: DEFAULT_CLIENT_OPTIONS,
  versionCacheTimoutInSeconds: 600,
};

export class JsonRpcProvider extends Provider {
  public endpoints: ApiEndpoints;
  protected client: JsonRpcClient;
  protected wsClient: WebsocketClient;
  private rpcApiVersion: RpcApiVersion | undefined;
  private cacheExpiry: number | undefined;
  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param endpoint URL to the Sui RPC endpoint, or a `Network` enum
   * @param options configuration options for the provider
   */
  constructor(
    endpoint: string | Network = Network.DEVNET,
    public options: RpcProviderOptions = DEFAULT_OPTIONS,
  ) {
    super();

    if ((Object.values(Network) as string[]).includes(endpoint)) {
      this.endpoints = NETWORK_TO_API[endpoint as Network];
    } else {
      this.endpoints = {
        fullNode: endpoint,
        faucet: options.faucetURL,
      };
    }

    const opts = { ...DEFAULT_OPTIONS, ...options };

    this.client = new JsonRpcClient(this.endpoints.fullNode);
    this.wsClient = new WebsocketClient(
      this.endpoints.fullNode,
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
        Date.now() + (this.options.versionCacheTimoutInSeconds ?? 0);
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
    if (!this.endpoints.faucet) {
      throw new Error('Faucet URL is not specified');
    }
    return requestSuiFromFaucet(this.endpoints.faucet, recipient, httpHeaders);
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
    // TODO: Add caching since package object does not change
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
    // TODO: Add caching since package object does not change
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
    // TODO: Add caching since package object does not change
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
  ): Promise<SuiObjectInfo[]> {
    try {
      if (!address || !isValidSuiAddress(normalizeSuiAddress(address))) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getObjectsOwnedByAddress',
        [address],
        GetOwnedObjectsResponse,
        this.options.skipDataValidation,
      );
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

  /**
   * @deprecated The method should not be used
   */
  async getCoinBalancesOwnedByAddress(
    address: SuiAddress,
    typeArg?: string,
  ): Promise<GetObjectDataResponse[]> {
    const objects = await this.getObjectsOwnedByAddress(address);
    const coinIds = objects
      .filter(
        (obj: SuiObjectInfo) =>
          Coin.isCoin(obj) &&
          (typeArg === undefined || typeArg === Coin.getCoinTypeArg(obj)),
      )
      .map((c) => c.objectId);

    return await this.getObjectBatch(coinIds);
  }

  async selectCoinsWithBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<GetObjectDataResponse[]> {
    const coinsStruct = await this.getCoins(address, typeArg);
    const coinIds = coinsStruct.data.map((c) => c.coinObjectId);
    const coins = await this.getObjectBatch(coinIds);
    return (await Coin.selectCoinsWithBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude,
    )) as GetObjectDataResponse[];
  }

  async selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<GetObjectDataResponse[]> {
    const coinsStruct = await this.getCoins(address, typeArg);
    const coinIds = coinsStruct.data.map((c) => c.coinObjectId);
    const coins = await this.getObjectBatch(coinIds);
    return (await Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude,
    )) as GetObjectDataResponse[];
  }

  async getObject(objectId: ObjectId): Promise<GetObjectDataResponse> {
    try {
      if (!objectId || !isValidSuiObjectId(normalizeSuiObjectId(objectId))) {
        throw new Error('Invalid Sui Object id');
      }
      return await this.client.requestWithType(
        'sui_getObject',
        [objectId],
        GetObjectDataResponse,
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
  ): Promise<GetObjectDataResponse[]> {
    try {
      const requests = objectIds.map((id) => {
        if (!id || !isValidSuiObjectId(normalizeSuiObjectId(id))) {
          throw new Error(`Invalid Sui Object id ${id}`);
        }
        return {
          method: 'sui_getObject',
          args: [id],
        };
      });
      return await this.client.batchRequestWithType(
        requests,
        GetObjectDataResponse,
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
  ): Promise<SuiExecuteTransactionResponse> {
    try {
      return await this.client.requestWithType(
        'sui_executeTransactionSerializedSig',
        [
          typeof txnBytes === 'string' ? txnBytes : toB64(txnBytes),
          signature,
          requestType,
        ],
        SuiExecuteTransactionResponse,
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

  async getTransactionAuthSigners(
    digest: TransactionDigest,
  ): Promise<SuiTransactionAuthSignersResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactionAuthSigners',
        [digest],
        SuiTransactionAuthSignersResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error fetching transaction auth signers: ${err}`);
    }
  }

  // Governance
  async getReferenceGasPrice(): Promise<number> {
    const version = await this.getRpcApiVersion();
    // TODO: clean up after 0.22.0 is deployed on both DevNet and TestNet
    if (version && lt(versionToString(version), '0.22.0')) {
      return 1;
    }
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
    tx: UnserializedSignableTransaction | string | Uint8Array,
    gasPrice: number | null = null,
    epoch: number | null = null,
  ): Promise<DevInspectResults> {
    try {
      const version = await this.getRpcApiVersion();
      // TODO: remove after 0.24.0 is deployed in both DevNet and TestNet
      if (version?.major === 0 && version?.minor < 24) {
        return this.devInspectTransactionDeprecated(sender, tx, epoch);
      }

      let devInspectTxBytes;
      if (typeof tx === 'string') {
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

  async devInspectTransactionDeprecated(
    sender: SuiAddress,
    tx: UnserializedSignableTransaction | string | Uint8Array,
    epoch: number | null = null,
  ): Promise<DevInspectResults> {
    let devInspectTxBytes;
    if (typeof tx === 'string') {
      devInspectTxBytes = tx;
    } else if (tx instanceof Uint8Array) {
      devInspectTxBytes = toB64(tx);
    } else {
      if (tx.kind === 'moveCall' && tx.data.gasBudget == null) {
        const moveCall = tx.data;
        const resp = await this.client.requestWithType(
          'sui_devInspectMoveCall',
          [
            sender,
            moveCall.packageObjectId,
            moveCall.module,
            moveCall.function,
            moveCall.typeArguments,
            moveCall.arguments,
          ],
          DevInspectResults,
          this.options.skipDataValidation,
        );
        return resp;
      }
      devInspectTxBytes = toB64(
        await new LocalTxnDataSerializer(this).serializeToBytes(sender, tx),
      );
    }

    const resp = await this.client.requestWithType(
      'sui_devInspectTransaction',
      [devInspectTxBytes, epoch],
      DevInspectResults,
      this.options.skipDataValidation,
    );
    return resp;
  }

  async dryRunTransaction(txBytes: Uint8Array): Promise<TransactionEffects> {
    try {
      const resp = await this.client.requestWithType(
        'sui_dryRunTransaction',
        [toB64(txBytes)],
        TransactionEffects,
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
    name: string,
  ): Promise<GetObjectDataResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getDynamicFieldObject',
        [parent_object_id, name],
        GetObjectDataResponse,
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
