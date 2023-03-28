// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorResponse, HttpHeaders, JsonRpcClient } from '../rpc/client';
import {
  ExecuteTransactionRequestType,
  ObjectId,
  PaginatedTransactionResponse,
  SubscriptionId,
  SuiAddress,
  SuiEventFilter,
  SuiMoveFunctionArgTypes,
  SuiMoveNormalizedFunction,
  SuiMoveNormalizedModule,
  SuiMoveNormalizedModules,
  SuiMoveNormalizedStruct,
  SuiTransactionBlockResponse,
  TransactionDigest,
  SuiTransactionBlockResponseQuery,
  RpcApiVersion,
  parseVersionFromString,
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
  DelegatedStake,
  CoinBalance,
  CoinSupply,
  CheckpointDigest,
  Checkpoint,
  CommitteeInfo,
  DryRunTransactionBlockResponse,
  SuiObjectDataOptions,
  SuiSystemStateSummary,
  SuiTransactionBlockResponseOptions,
  SuiEvent,
  PaginatedObjectsResponse,
  SuiObjectResponseQuery,
  CheckpointedObjectId,
} from '../types';
import { DynamicFieldName, DynamicFieldPage } from '../types/dynamic_fields';
import {
  DEFAULT_CLIENT_OPTIONS,
  WebsocketClient,
  WebsocketClientOptions,
} from '../rpc/websocket-client';
import { requestSuiFromFaucet } from '../rpc/faucet-client';
import { any, is, array, string } from 'superstruct';
import { toB64 } from '@mysten/bcs';
import { SerializedSignature } from '../cryptography/signature';
import { Connection, devnetConnection } from '../rpc/connection';
import { TransactionBlock } from '../builder';
import { CheckpointPage } from '../types/checkpoints';
import { RPCError } from '../utils/errors';

export const TARGETED_RPC_VERSION = '0.29.0';

export interface PaginationArguments {
  /** Optional paging cursor */
  cursor?: CheckpointedObjectId | ObjectId | null;
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
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }

    return await this.client.requestWithType(
      'suix_getCoins',
      [input.owner, input.coinType, input.cursor, input.limit],
      PaginatedCoins,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get all Coin objects owned by an address.
   */
  async getAllCoins(
    input: {
      owner: SuiAddress;
    } & PaginationArguments,
  ): Promise<PaginatedCoins> {
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }

    return await this.client.requestWithType(
      'suix_getAllCoins',
      [input.owner, input.cursor, input.limit],
      PaginatedCoins,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get the total coin balance for one coin type, owned by the address owner.
   */
  async getBalance(input: {
    owner: SuiAddress;
    /** optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified. */
    coinType?: string | null;
  }): Promise<CoinBalance> {
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }
    return await this.client.requestWithType(
      'suix_getBalance',
      [input.owner, input.coinType],
      CoinBalance,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get the total coin balance for all coin type, owned by the address owner.
   */
  async getAllBalances(input: { owner: SuiAddress }): Promise<CoinBalance[]> {
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }
    return await this.client.requestWithType(
      'suix_getAllBalances',
      [input.owner],
      array(CoinBalance),
      this.options.skipDataValidation,
    );
  }

  /**
   * Fetch CoinMetadata for a given coin type
   */
  async getCoinMetadata(input: { coinType: string }): Promise<CoinMetadata> {
    return await this.client.requestWithType(
      'suix_getCoinMetadata',
      [input.coinType],
      CoinMetadataStruct,
      this.options.skipDataValidation,
    );
  }

  /**
   *  Fetch total supply for a coin
   */
  async getTotalSupply(input: { coinType: string }): Promise<CoinSupply> {
    return await this.client.requestWithType(
      'suix_getTotalSupply',
      [input.coinType],
      CoinSupply,
      this.options.skipDataValidation,
    );
  }

  /**
   * Invoke any RPC method
   * @param method the method to be invoked
   * @param args the arguments to be passed to the RPC request
   */
  async call(method: string, args: Array<any>): Promise<any> {
    const response = await this.client.request(method, args);
    if (is(response, ErrorResponse)) {
      throw new RPCError({
        req: { method, args },
        code: response.error.code,
        data: response.error.data,
        cause: new Error(response.error.message),
      });
    }
    return response.result;
  }

  /**
   * Get Move function argument types like read, write and full access
   */
  async getMoveFunctionArgTypes(input: {
    package: string;
    module: string;
    function: string;
  }): Promise<SuiMoveFunctionArgTypes> {
    return await this.client.requestWithType(
      'sui_getMoveFunctionArgTypes',
      [input.package, input.module, input.function],
      SuiMoveFunctionArgTypes,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get a map from module name to
   * structured representations of Move modules
   */
  async getNormalizedMoveModulesByPackage(input: {
    package: string;
  }): Promise<SuiMoveNormalizedModules> {
    return await this.client.requestWithType(
      'sui_getNormalizedMoveModulesByPackage',
      [input.package],
      SuiMoveNormalizedModules,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get a structured representation of Move module
   */
  async getNormalizedMoveModule(input: {
    package: string;
    module: string;
  }): Promise<SuiMoveNormalizedModule> {
    return await this.client.requestWithType(
      'sui_getNormalizedMoveModule',
      [input.package, input.module],
      SuiMoveNormalizedModule,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get a structured representation of Move function
   */
  async getNormalizedMoveFunction(input: {
    package: string;
    module: string;
    function: string;
  }): Promise<SuiMoveNormalizedFunction> {
    return await this.client.requestWithType(
      'sui_getNormalizedMoveFunction',
      [input.package, input.module, input.function],
      SuiMoveNormalizedFunction,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get a structured representation of Move struct
   */
  async getNormalizedMoveStruct(input: {
    package: string;
    module: string;
    struct: string;
  }): Promise<SuiMoveNormalizedStruct> {
    return await this.client.requestWithType(
      'sui_getNormalizedMoveStruct',
      [input.package, input.module, input.struct],
      SuiMoveNormalizedStruct,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get all objects owned by an address
   */
  async getOwnedObjects(
    input: {
      owner: SuiAddress;
    } & PaginationArguments &
      SuiObjectResponseQuery,
  ): Promise<PaginatedObjectsResponse> {
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }

    return await this.client.requestWithType(
      'suix_getOwnedObjects',
      [
        input.owner,
        {
          filter: input.filter,
          options: input.options,
        } as SuiObjectResponseQuery,
        input.cursor,
        input.limit,
      ],
      PaginatedObjectsResponse,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get details about an object
   */
  async getObject(input: {
    id: ObjectId;
    options?: SuiObjectDataOptions;
  }): Promise<SuiObjectResponse> {
    if (!input.id || !isValidSuiObjectId(normalizeSuiObjectId(input.id))) {
      throw new Error('Invalid Sui Object id');
    }
    return await this.client.requestWithType(
      'sui_getObject',
      [input.id, input.options],
      SuiObjectResponse,
      this.options.skipDataValidation,
    );
  }

  /**
   * Batch get details about a list of objects. If any of the object ids are duplicates the call will fail
   */
  async multiGetObjects(input: {
    ids: ObjectId[];
    options?: SuiObjectDataOptions;
  }): Promise<SuiObjectResponse[]> {
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
  }

  /**
   * Get transaction blocks for a given query criteria
   */
  async queryTransactionBlocks(
    input: SuiTransactionBlockResponseQuery &
      PaginationArguments &
      OrderArguments,
  ): Promise<PaginatedTransactionResponse> {
    return await this.client.requestWithType(
      'suix_queryTransactionBlocks',
      [
        {
          filter: input.filter,
          options: input.options,
        } as SuiTransactionBlockResponseQuery,
        input.cursor,
        input.limit,
        (input.order || 'descending') === 'descending',
      ],
      PaginatedTransactionResponse,
      this.options.skipDataValidation,
    );
  }

  async getTransactionBlock(input: {
    digest: TransactionDigest;
    options?: SuiTransactionBlockResponseOptions;
  }): Promise<SuiTransactionBlockResponse> {
    if (!isValidTransactionDigest(input.digest)) {
      throw new Error('Invalid Transaction digest');
    }
    return await this.client.requestWithType(
      'sui_getTransactionBlock',
      [input.digest, input.options],
      SuiTransactionBlockResponse,
      this.options.skipDataValidation,
    );
  }

  async multiGetTransactionBlocks(input: {
    digests: TransactionDigest[];
    options?: SuiTransactionBlockResponseOptions;
  }): Promise<SuiTransactionBlockResponse[]> {
    input.digests.forEach((d) => {
      if (!isValidTransactionDigest(d)) {
        throw new Error(`Invalid Transaction digest ${d}`);
      }
    });

    const hasDuplicates = input.digests.length !== new Set(input.digests).size;
    if (hasDuplicates) {
      throw new Error(`Duplicate digests in batch call ${input.digests}`);
    }

    return await this.client.requestWithType(
      'sui_multiGetTransactionBlocks',
      [input.digests, input.options],
      array(SuiTransactionBlockResponse),
      this.options.skipDataValidation,
    );
  }

  async executeTransactionBlock(input: {
    transactionBlock: Uint8Array | string;
    signature: SerializedSignature | SerializedSignature[];
    options?: SuiTransactionBlockResponseOptions;
    requestType?: ExecuteTransactionRequestType;
  }): Promise<SuiTransactionBlockResponse> {
    return await this.client.requestWithType(
      'sui_executeTransactionBlock',
      [
        typeof input.transactionBlock === 'string'
          ? input.transactionBlock
          : toB64(input.transactionBlock),
        Array.isArray(input.signature) ? input.signature : [input.signature],
        input.options,
        input.requestType,
      ],
      SuiTransactionBlockResponse,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get total number of transactions
   */

  async getTotalTransactionBlocks(): Promise<bigint> {
    const resp = await this.client.requestWithType(
      'sui_getTotalTransactionBlocks',
      [],
      string(),
      this.options.skipDataValidation,
    );
    return BigInt(resp);
  }

  /**
   * Getting the reference gas price for the network
   */
  async getReferenceGasPrice(): Promise<bigint> {
    const resp = await this.client.requestWithType(
      'suix_getReferenceGasPrice',
      [],
      string(),
      this.options.skipDataValidation,
    );
    return BigInt(resp);
  }

  /**
   * Return the delegated stakes for an address
   */
  async getStakes(input: { owner: SuiAddress }): Promise<DelegatedStake[]> {
    if (!input.owner || !isValidSuiAddress(normalizeSuiAddress(input.owner))) {
      throw new Error('Invalid Sui address');
    }
    return await this.client.requestWithType(
      'suix_getStakes',
      [input.owner],
      array(DelegatedStake),
      this.options.skipDataValidation,
    );
  }

  /**
   * Return the delegated stakes queried by id.
   */
  async getStakesByIds(input: {
    stakedSuiIds: ObjectId[];
  }): Promise<DelegatedStake[]> {
    input.stakedSuiIds.forEach((id) => {
      if (!id || !isValidSuiObjectId(normalizeSuiObjectId(id))) {
        throw new Error(`Invalid Sui Stake id ${id}`);
      }
    });
    return await this.client.requestWithType(
      'suix_getStakesByIds',
      [input.stakedSuiIds],
      array(DelegatedStake),
      this.options.skipDataValidation,
    );
  }

  /**
   * Return the latest system state content.
   */
  async getLatestSuiSystemState(): Promise<SuiSystemStateSummary> {
    return await this.client.requestWithType(
      'suix_getLatestSuiSystemState',
      [],
      SuiSystemStateSummary,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get events for a given query criteria
   */
  async queryEvents(
    input: {
      /** the event query criteria. */
      query: SuiEventFilter;
    } & PaginationArguments &
      OrderArguments,
  ): Promise<PaginatedEvents> {
    return await this.client.requestWithType(
      'suix_queryEvents',
      [
        input.query,
        input.cursor,
        input.limit,
        (input.order || 'descending') === 'descending',
      ],
      PaginatedEvents,
      this.options.skipDataValidation,
    );
  }

  /**
   * Subscribe to get notifications whenever an event matching the filter occurs
   */
  async subscribeEvent(input: {
    /** filter describing the subset of events to follow */
    filter: SuiEventFilter;
    /** function to run when we receive a notification of a new event matching the filter */
    onMessage: (event: SuiEvent) => void;
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
   * Runs the transaction block in dev-inpsect mode. Which allows for nearly any
   * transaction (or Move call) with any arguments. Detailed results are
   * provided, including both the transaction effects and any return values.
   */
  async devInspectTransactionBlock(input: {
    transactionBlock: TransactionBlock | string | Uint8Array;
    sender: SuiAddress;
    /** Default to use the network reference gas price stored in the Sui System State object */
    gasPrice?: bigint | number | null;
    /** optional. Default to use the current epoch number stored in the Sui System State object */
    epoch?: number | null;
  }): Promise<DevInspectResults> {
    let devInspectTxBytes;
    if (TransactionBlock.is(input.transactionBlock)) {
      input.transactionBlock.setSenderIfNotSet(input.sender);
      devInspectTxBytes = toB64(
        await input.transactionBlock.build({
          provider: this,
          onlyTransactionKind: true,
        }),
      );
    } else if (typeof input.transactionBlock === 'string') {
      devInspectTxBytes = input.transactionBlock;
    } else if (input.transactionBlock instanceof Uint8Array) {
      devInspectTxBytes = toB64(input.transactionBlock);
    } else {
      throw new Error('Unknown transaction block format.');
    }

    return await this.client.requestWithType(
      'sui_devInspectTransactionBlock',
      [input.sender, devInspectTxBytes, input.gasPrice, input.epoch],
      DevInspectResults,
      this.options.skipDataValidation,
    );
  }

  /**
   * Dry run a transaction block and return the result.
   */
  async dryRunTransactionBlock(input: {
    transactionBlock: Uint8Array | string;
  }): Promise<DryRunTransactionBlockResponse> {
    return await this.client.requestWithType(
      'sui_dryRunTransactionBlock',
      [
        typeof input.transactionBlock === 'string'
          ? input.transactionBlock
          : toB64(input.transactionBlock),
      ],
      DryRunTransactionBlockResponse,
      this.options.skipDataValidation,
    );
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
    if (
      !input.parentId ||
      !isValidSuiObjectId(normalizeSuiObjectId(input.parentId))
    ) {
      throw new Error('Invalid Sui Object id');
    }
    return await this.client.requestWithType(
      'suix_getDynamicFields',
      [input.parentId, input.cursor, input.limit],
      DynamicFieldPage,
      this.options.skipDataValidation,
    );
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
    return await this.client.requestWithType(
      'suix_getDynamicFieldObject',
      [input.parentId, input.name],
      SuiObjectResponse,
      this.options.skipDataValidation,
    );
  }

  /**
   * Get the sequence number of the latest checkpoint that has been executed
   */
  async getLatestCheckpointSequenceNumber(): Promise<string> {
    const resp = await this.client.requestWithType(
      'sui_getLatestCheckpointSequenceNumber',
      [],
      string(),
      this.options.skipDataValidation,
    );
    return String(resp);
  }

  /**
   * Returns information about a given checkpoint
   */
  async getCheckpoint(input: {
    /** The checkpoint digest or sequence number */
    id: CheckpointDigest | string;
  }): Promise<Checkpoint> {
    return await this.client.requestWithType(
      'sui_getCheckpoint',
      [input.id],
      Checkpoint,
      this.options.skipDataValidation,
    );
  }

  /**
   * Returns historical checkpoints paginated
   */
  async getCheckpoints(input: {
    /**
     * An optional paging cursor. If provided, the query will start from the next item after the specified cursor.
     * Default to start from the first item if not specified.
     */
    cursor?: string;
    /** Maximum item returned per page, default to 100 if not specified. */
    limit?: number;
    /** query result ordering, default to false (ascending order), oldest record first */
    descendingOrder: boolean;
  }): Promise<CheckpointPage> {
    const resp = await this.client.requestWithType(
      'sui_getCheckpoints',
      [input.cursor, input.limit, input.descendingOrder],
      CheckpointPage,
      this.options.skipDataValidation,
    );
    return resp;
  }

  /**
   * Return the committee information for the asked epoch
   */
  async getCommitteeInfo(input?: {
    /** The epoch of interest. If null, default to the latest epoch */
    epoch?: number;
  }): Promise<CommitteeInfo> {
    return await this.client.requestWithType(
      'suix_getCommitteeInfo',
      [input?.epoch],
      CommitteeInfo,
    );
  }
}
