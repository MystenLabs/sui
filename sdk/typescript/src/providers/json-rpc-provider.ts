// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorResponse, HttpHeaders, JsonRpcClient } from '../rpc/client';
import {
  Coin,
  ExecuteTransactionRequestType,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  ObjectId,
  PaginatedTransactionResponse,
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
  SuiTransactionResponse,
  TransactionDigest,
  SuiTransactionResponseQuery,
  SUI_TYPE_ARG,
  RpcApiVersion,
  parseVersionFromString,
  EventQuery,
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
  CoinBalance,
  CoinSupply,
  CheckpointDigest,
  Checkpoint,
  CommitteeInfo,
  DryRunTransactionResponse,
  SuiObjectDataOptions,
  SuiSystemStateSummary,
  CoinStruct,
  SuiTransactionResponseOptions,
} from '../types';
import { DynamicFieldName, DynamicFieldPage } from '../types/dynamic_fields';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';
import { requestSuiFromFaucet } from '../rpc/faucet-client';
import { any, is, number, array } from 'superstruct';
import { toB64 } from '@mysten/bcs';
import { SerializedSignature } from '../cryptography/signature';
import { Connection, devnetConnection } from '../rpc/connection';
import { Transaction } from '../builder';

export const TARGETED_RPC_VERSION = '0.27.0';

export interface PaginationArguments {
  /** Optional paging cursor */
  cursor?: ObjectId | null;
  /** Maximum item returned per page */
  limit?: number | null;
}

export interface OrderArguments {
  order?: Order | null;
}

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

export class JsonRpcProvider {
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

  /**
   * Get all Coin<`coin_type`> objects owned by an address.
   */
  async getCoins(input: {
    owner: SuiAddress;
    coinType?: string | null;
    cursor?: ObjectId | null;
    limit?: number | null;
  }): Promise<PaginatedCoins> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }

      return await this.client.requestWithType(
        'sui_getCoins',
        [input.owner, input.coinType, input.cursor, input.limit],
        PaginatedCoins,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error getting coins for owner ${input.owner}: ${err}`);
    }
  }

  /**
   * Get all Coin objects owned by an address.
   */
  async getAllCoins(
    input: {
      owner: SuiAddress;
    } & PaginationArguments,
  ): Promise<PaginatedCoins> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }

      return await this.client.requestWithType(
        'sui_getAllCoins',
        [input.owner, input.cursor, input.limit],
        PaginatedCoins,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting all coins for owner ${input.owner}: ${err}`,
      );
    }
  }

  /**
   * Get the total coin balance for one coin type, owned by the address owner.
   */
  async getBalance(input: {
    owner: SuiAddress;
    /** optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified. */
    coinType?: string | null;
  }): Promise<CoinBalance> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getBalance',
        [input.owner, input.coinType],
        CoinBalance,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting balance for coin type ${input.coinType} for owner ${input.owner}: ${err}`,
      );
    }
  }

  /**
   * Get the total coin balance for all coin type, owned by the address owner.
   */
  async getAllBalances(input: { owner: SuiAddress }): Promise<CoinBalance[]> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }
      return await this.client.requestWithType(
        'sui_getAllBalances',
        [input.owner],
        array(CoinBalance),
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting all balances for owner ${input.owner}: ${err}`,
      );
    }
  }

  /**
   * Fetch CoinMetadata for a given coin type
   */
  async getCoinMetadata(input: { coinType: string }): Promise<CoinMetadata> {
    try {
      return await this.client.requestWithType(
        'sui_getCoinMetadata',
        [input.coinType],
        CoinMetadataStruct,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching CoinMetadata for ${input.coinType}: ${err}`,
      );
    }
  }

  /**
   *  Fetch total supply for a coin
   */
  async getTotalSupply(input: { coinType: string }): Promise<CoinSupply> {
    try {
      return await this.client.requestWithType(
        'sui_getTotalSupply',
        [input.coinType],
        CoinSupply,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching total supply for Coin type ${input.coinType}: ${err}`,
      );
    }
  }

  /**
   * Invoke any RPC endpoint
   * @param endpoint the endpoint to be invoked
   * @param params the arguments to be passed to the RPC request
   */
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

  /**
   * Get Move function argument types like read, write and full access
   */
  async getMoveFunctionArgTypes(input: {
    package: string;
    module: string;
    function: string;
  }): Promise<SuiMoveFunctionArgTypes> {
    try {
      return await this.client.requestWithType(
        'sui_getMoveFunctionArgTypes',
        [input.package, input.module, input.function],
        SuiMoveFunctionArgTypes,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching Move function arg types with package object ID: ${input.package}, module name: ${input.module}, function name: ${input.function}`,
      );
    }
  }

  /**
   * Get a map from module name to
   * structured representations of Move modules
   */
  async getNormalizedMoveModulesByPackage(input: {
    package: string;
  }): Promise<SuiMoveNormalizedModules> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModulesByPackage',
        [input.package],
        SuiMoveNormalizedModules,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching package: ${err} for package ${input.package}`,
      );
    }
  }

  /**
   * Get a structured representation of Move module
   */
  async getNormalizedMoveModule(input: {
    package: string;
    module: string;
  }): Promise<SuiMoveNormalizedModule> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModule',
        [input.package, input.module],
        SuiMoveNormalizedModule,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching module: ${err} for package ${input.package}, module ${input.module}`,
      );
    }
  }

  /**
   * Get a structured representation of Move function
   */
  async getNormalizedMoveFunction(input: {
    package: string;
    module: string;
    function: string;
  }): Promise<SuiMoveNormalizedFunction> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveFunction',
        [input.package, input.module, input.function],
        SuiMoveNormalizedFunction,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching function: ${err} for package ${input.package}, module ${input.module} and function ${input.function}`,
      );
    }
  }

  /**
   * Get a structured representation of Move struct
   */
  async getNormalizedMoveStruct(input: {
    package: string;
    module: string;
    struct: string;
  }): Promise<SuiMoveNormalizedStruct> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveStruct',
        [input.package, input.module, input.struct],
        SuiMoveNormalizedStruct,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching struct: ${err} for package ${input.package}, module ${input.module} and struct ${input.struct}`,
      );
    }
  }

  /**
   * Get all objects owned by an address
   */
  async getObjectsOwnedByAddress(input: {
    owner: SuiAddress;
    /** a fully qualified type name for the object(e.g., 0x2::coin::Coin<0x2::sui::SUI>)
     * or type name without generics (e.g., 0x2::coin::Coin will match all 0x2::coin::Coin<T>) */
    typeFilter?: string;
  }): Promise<SuiObjectInfo[]> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }
      const objects = await this.client.requestWithType(
        'sui_getObjectsOwnedByAddress',
        [input.owner],
        GetOwnedObjectsResponse,
        this.options.skipDataValidation,
      );
      // TODO: remove this once we migrated to the new queryObject API
      if (input.typeFilter) {
        return objects.filter(
          (obj: SuiObjectInfo) =>
            obj.type === input.typeFilter ||
            obj.type.startsWith(input.typeFilter + '<'),
        );
      }
      return objects;
    } catch (err) {
      throw new Error(
        `Error fetching owned object: ${err} for address ${input.owner}`,
      );
    }
  }

  /** @deprecated */
  async selectCoinsWithBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<CoinStruct[]> {
    const coinsStruct = await this.getCoins({
      owner: address,
      coinType: typeArg,
    });
    return Coin.selectCoinsWithBalanceGreaterThanOrEqual(
      coinsStruct.data,
      amount,
      exclude,
    );
  }

  /** @deprecated */
  async selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    address: SuiAddress,
    amount: bigint,
    typeArg: string = SUI_TYPE_ARG,
    exclude: ObjectId[] = [],
  ): Promise<CoinStruct[]> {
    const coinsStruct = await this.getCoins({
      owner: address,
      coinType: typeArg,
    });
    const coins = coinsStruct.data;

    return Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
      coins,
      amount,
      exclude,
    );
  }

  /**
   * Get details about an object
   */
  async getObject(input: {
    id: ObjectId;
    options?: SuiObjectDataOptions;
  }): Promise<SuiObjectResponse> {
    try {
      if (!input.id || !isValidSuiObjectId(normalizeSuiObjectId(input.id))) {
        throw new Error('Invalid Sui Object id');
      }
      return await this.client.requestWithType(
        'sui_getObject',
        [input.id, input.options],
        SuiObjectResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${input.id}`);
    }
  }

  /**
   * Batch get details about a list of objects. If any of the object ids are duplicates the call will fail
   */
  async multiGetObjects(input: {
    ids: ObjectId[];
    options?: SuiObjectDataOptions;
  }): Promise<SuiObjectResponse[]> {
    try {
      input.ids.forEach((id) => {
        if (!id || !isValidSuiObjectId(normalizeSuiObjectId(id))) {
          throw new Error(`Invalid Sui Object id ${id}`);
        }
      });
      const hasDuplicates = input.ids.length !== new Set(input.ids).size;
      if (hasDuplicates) {
        throw new Error(`Duplicate object ids in batch call ${input.ids}`);
      }

      return await this.client.requestWithType(
        'sui_multiGetObjects',
        [input.ids, input.options],
        array(SuiObjectResponse),
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error fetching object info: ${err} for ids [${input.ids}]`,
      );
    }
  }

  /**
   * Get transactions for a given query criteria
   */
  async queryTransactions(
    input: SuiTransactionResponseQuery & PaginationArguments & OrderArguments,
  ): Promise<PaginatedTransactionResponse> {
    try {
      return await this.client.requestWithType(
        'sui_queryTransactions',
        [
          {
            filter: input.filter,
            options: input.options,
          } as SuiTransactionResponseQuery,
          input.cursor,
          input.limit,
          (input.order || 'descending') === 'descending',
        ],
        PaginatedTransactionResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting transactions for query: ${err} for filter ${input.filter}`,
      );
    }
  }

  /**
   * @deprecated this method will be removed by April 2023.
   * Use `queryTransactions` instead
   */
  async queryTransactionsForObjectDeprecated(
    objectID: ObjectId,
    descendingOrder: boolean = true,
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_queryTransactions',
        args: [
          { filter: { InputObject: objectID } },
          null,
          null,
          descendingOrder,
        ],
      },
      {
        method: 'sui_queryTransactions',
        args: [
          { filter: { MutatedObject: objectID } },
          null,
          null,
          descendingOrder,
        ],
      },
    ];

    try {
      if (!objectID || !isValidSuiObjectId(normalizeSuiObjectId(objectID))) {
        throw new Error('Invalid Sui Object id');
      }
      const results = await this.client.batchRequestWithType(
        requests,
        PaginatedTransactionResponse,
        this.options.skipDataValidation,
      );
      return [
        ...results[0].data.map((r) => r.digest),
        ...results[1].data.map((r) => r.digest),
      ];
    } catch (err) {
      throw new Error(
        `Error getting transactions for object: ${err} for id ${objectID}`,
      );
    }
  }

  /**
   * @deprecated this method will be removed by April 2023.
   * Use `queryTransactions` instead
   */
  async queryTransactionsForAddressDeprecated(
    addressID: SuiAddress,
    descendingOrder: boolean = true,
  ): Promise<GetTxnDigestsResponse> {
    const requests = [
      {
        method: 'sui_queryTransactions',
        args: [
          { filter: { ToAddress: addressID } },
          null,
          null,
          descendingOrder,
        ],
      },
      {
        method: 'sui_queryTransactions',
        args: [
          { filter: { FromAddress: addressID } },
          null,
          null,
          descendingOrder,
        ],
      },
    ];
    try {
      if (!addressID || !isValidSuiAddress(normalizeSuiAddress(addressID))) {
        throw new Error('Invalid Sui address');
      }
      const results = await this.client.batchRequestWithType(
        requests,
        PaginatedTransactionResponse,
        this.options.skipDataValidation,
      );
      return [
        ...results[0].data.map((r) => r.digest),
        ...results[1].data.map((r) => r.digest),
      ];
    } catch (err) {
      throw new Error(
        `Error getting transactions for address: ${err} for id ${addressID}`,
      );
    }
  }

  async getTransaction(input: {
    digest: TransactionDigest;
    options?: SuiTransactionResponseOptions;
  }): Promise<SuiTransactionResponse> {
    try {
      if (!isValidTransactionDigest(input.digest)) {
        throw new Error('Invalid Transaction digest');
      }
      const resp = await this.client.requestWithType(
        'sui_getTransaction',
        [input.digest, input.options],
        SuiTransactionResponse,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting transaction with effects: ${err} for digest ${input.digest}`,
      );
    }
  }

  async multiGetTransactions(input: {
    digests: TransactionDigest[];
    options?: SuiTransactionResponseOptions;
  }): Promise<SuiTransactionResponse[]> {
    try {
      input.digests.forEach((d) => {
        if (!isValidTransactionDigest(d)) {
          throw new Error(`Invalid Transaction digest ${d}`);
        }
      });

      const hasDuplicates =
        input.digests.length !== new Set(input.digests).size;
      if (hasDuplicates) {
        throw new Error(`Duplicate digests in batch call ${input.digests}`);
      }

      return await this.client.requestWithType(
        'sui_multiGetTransactions',
        [input.digests, input.options],
        array(SuiTransactionResponse),
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting transaction effects: ${err} for digests [${input.digests}]`,
      );
    }
  }

  async executeTransaction(input: {
    transaction: Uint8Array | string;
    signature: SerializedSignature | SerializedSignature[];
    options?: SuiTransactionResponseOptions;
    requestType?: ExecuteTransactionRequestType;
  }): Promise<SuiTransactionResponse> {
    try {
      return await this.client.requestWithType(
        'sui_executeTransaction',
        [
          typeof input.transaction === 'string'
            ? input.transaction
            : toB64(input.transaction),
          Array.isArray(input.signature) ? input.signature : [input.signature],
          input.options,
          input.requestType,
        ],
        SuiTransactionResponse,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(`Error executing transaction with request type: ${err}`);
    }
  }

  /**
   * Get total number of transactions
   */
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

  /** @deprecated */
  async getTransactionDigestsInRangeDeprecated(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber,
  ): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getTransactionsInRangeDeprecated',
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

  /**
   * Getting the reference gas price for the network
   */
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

  /**
   * Return the delegated stakes for an address
   */
  async getDelegatedStakes(input: {
    owner: SuiAddress;
  }): Promise<DelegatedStake[]> {
    try {
      if (
        !input.owner ||
        !isValidSuiAddress(normalizeSuiAddress(input.owner))
      ) {
        throw new Error('Invalid Sui address');
      }
      const resp = await this.client.requestWithType(
        'sui_getDelegatedStakes',
        [input.owner],
        array(DelegatedStake),
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(`Error in getDelegatedStakes: ${err}`);
    }
  }

  /**
   * Return the latest system state content.
   */
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

  /**
   * Get events for a given query criteria
   */
  async getEvents(
    input: {
      /** the event query criteria. */
      query: EventQuery;
    } & PaginationArguments &
      OrderArguments,
  ): Promise<PaginatedEvents> {
    try {
      return await this.client.requestWithType(
        'sui_getEvents',
        [
          input.query,
          input.cursor,
          input.limit,
          (input.order || 'descending') === 'descending',
        ],
        PaginatedEvents,
        this.options.skipDataValidation,
      );
    } catch (err) {
      throw new Error(
        `Error getting events for query: ${err} for query ${input.query}`,
      );
    }
  }

  /**
   * Subscribe to get notifications whenever an event matching the filter occurs
   */
  async subscribeEvent(input: {
    /** filter describing the subset of events to follow */
    filter: SuiEventFilter;
    /** function to run when we receive a notification of a new event matching the filter */
    onMessage: (event: SuiEventEnvelope) => void;
  }): Promise<SubscriptionId> {
    return this.wsClient.subscribeEvent(input.filter, input.onMessage);
  }

  /**
   * Unsubscribe from an event subscription
   */
  async unsubscribeEvent(input: {
    /** subscription id to unsubscribe from (previously received from subscribeEvent)*/
    id: SubscriptionId;
  }): Promise<boolean> {
    return this.wsClient.unsubscribeEvent(input.id);
  }

  /**
   * Runs the transaction in dev-inpsect mode. Which allows for nearly any
   * transaction (or Move call) with any arguments. Detailed results are
   * provided, including both the transaction effects and any return values.
   */
  async devInspectTransaction(input: {
    transaction: Transaction | string | Uint8Array;
    sender: SuiAddress;
    /** Default to use the network reference gas price stored in the Sui System State object */
    gasPrice?: number | null;
    /** optional. Default to use the current epoch number stored in the Sui System State object */
    epoch?: number | null;
  }): Promise<DevInspectResults> {
    try {
      let devInspectTxBytes;
      if (Transaction.is(input.transaction)) {
        input.transaction.setSender(input.sender);
        devInspectTxBytes = toB64(
          await input.transaction.build({
            provider: this,
            onlyTransactionKind: true,
          }),
        );
      } else if (typeof input.transaction === 'string') {
        devInspectTxBytes = input.transaction;
      } else if (input.transaction instanceof Uint8Array) {
        devInspectTxBytes = toB64(input.transaction);
      } else {
        throw new Error('Unknown transaction format.');
      }

      const resp = await this.client.requestWithType(
        'sui_devInspectTransaction',
        [input.sender, devInspectTxBytes, input.gasPrice, input.epoch],
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

  /**
   * Dry run a transaction and return the result.
   */
  async dryRunTransaction(input: {
    transaction: Uint8Array | string;
  }): Promise<DryRunTransactionResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_dryRunTransaction',
        [
          typeof input.transaction === 'string'
            ? input.transaction
            : toB64(input.transaction),
        ],
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

  /**
   * Return the list of dynamic field objects owned by an object
   */
  async getDynamicFields(
    input: {
      /** The id of the parent object */
      parentId: ObjectId;
    } & PaginationArguments,
  ): Promise<DynamicFieldPage> {
    try {
      if (
        !input.parentId ||
        !isValidSuiObjectId(normalizeSuiObjectId(input.parentId))
      ) {
        throw new Error('Invalid Sui Object id');
      }
      const resp = await this.client.requestWithType(
        'sui_getDynamicFields',
        [input.parentId, input.cursor, input.limit],
        DynamicFieldPage,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting dynamic fields with request type: ${err} for parentId: ${input.parentId}, cursor: ${input.cursor} and limit: ${input.limit}.`,
      );
    }
  }

  /**
   * Return the dynamic field object information for a specified object
   */
  async getDynamicFieldObject(input: {
    /** The ID of the quered parent object */
    parentId: ObjectId;
    /** The name of the dynamic field */
    name: string | DynamicFieldName;
  }): Promise<SuiObjectResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getDynamicFieldObject',
        [input.parentId, input.name],
        SuiObjectResponse,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting dynamic field object with request type: ${err} for parent_object_id: ${input.parentId} and name: ${input.name}.`,
      );
    }
  }

  /**
   * Get the sequence number of the latest checkpoint that has been executed
   */
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

  /**
   * Returns information about a given checkpoint
   */
  async getCheckpoint(input: {
    /** The checkpoint digest or sequence number */
    id: CheckpointDigest | number;
  }): Promise<Checkpoint> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getCheckpoint',
        [input.id],
        Checkpoint,
        this.options.skipDataValidation,
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting checkpoint with request type: ${err} for id: ${input.id}.`,
      );
    }
  }

  /**
   * Return the committee information for the asked epoch
   */
  async getCommitteeInfo(input?: {
    /** The epoch of interest. If null, default to the latest epoch */
    epoch?: number;
  }): Promise<CommitteeInfo> {
    try {
      const committeeInfo = await this.client.requestWithType(
        'sui_getCommitteeInfo',
        [input?.epoch],
        CommitteeInfo,
      );

      return committeeInfo;
    } catch (error) {
      throw new Error(`Error getCommitteeInfo : ${error}`);
    }
  }
}
