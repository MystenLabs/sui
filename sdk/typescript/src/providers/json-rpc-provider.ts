// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider } from './provider';
import { JsonRpcClient } from '../rpc/client';
import {
  isGetObjectDataResponse,
  isGetOwnedObjectsResponse,
  isGetTxnDigestsResponse,
  isSuiTransactionResponse,
  isSuiMoveFunctionArgTypes,
  isSuiMoveNormalizedModules,
  isSuiMoveNormalizedModule,
  isSuiMoveNormalizedFunction,
  isSuiMoveNormalizedStruct,
  isSubscriptionEvent,
} from '../index.guard';
import {
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  GetObjectDataResponse,
  SuiObjectInfo,
  SuiMoveFunctionArgTypes,
  SuiMoveNormalizedModules,
  SuiMoveNormalizedModule,
  SuiMoveNormalizedFunction,
  SuiMoveNormalizedStruct,
  TransactionDigest,
  SuiTransactionResponse,
  SuiObjectRef,
  getObjectReference,
  Coin,
  SuiEventFilter,
  SuiEventEnvelope,
  SubscriptionId,
} from '../types';
import { SignatureScheme } from '../cryptography/publickey';
import { Client as WsRpcClient} from 'rpc-websockets';

const isNumber = (val: any): val is number => typeof val === 'number';
const isAny = (_val: any): _val is any => true;

export const getWebsocketUrl = (httpUrl: string, port?: number): string => {
  const url = new URL(httpUrl);
  url.protocol = url.protocol.replace('http', 'ws');
  url.port = (port ?? 9001).toString();
  return url.toString();
};

enum ConnectionState {
  NotConnected,
  Connecting,
  Connected
}

type JsonRpcMethodMessage<T> = {
  jsonrpc: '2.0',
  method: string,
  params: T
}

type FilterSubHandler = {
  id: SubscriptionId,
  onMessage: (event: SuiEventEnvelope) => void,
  filter: SuiEventFilter
};

type SubscriptionData = {
  filter: SuiEventFilter,
  onMessage: (event: SuiEventEnvelope) => void
}

export type SubscriptionEvent = { subscription: SubscriptionId, result: SuiEventEnvelope };

export class JsonRpcProvider extends Provider {
  protected wsClient: WsRpcClient;
  protected wsConnectionState: ConnectionState = ConnectionState.NotConnected;
  protected wsEndpoint: string;
  protected wsTimeout: number | null = null;
  protected wsSetup: boolean = false;

  protected activeSubscriptions: Map<SubscriptionId, SubscriptionData> = new Map();

  protected client: JsonRpcClient;
  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param endpoint URL to the Sui RPC endpoint
   * @param skipDataValidation default to `false`. If set to `true`, the rpc
   * client will not check if the responses from the RPC server conform to the schema
   * defined in the TypeScript SDK. The mismatches often happen when the SDK
   * is in a different version than the RPC server. Skipping the validation
   * can maximize the version compatibility of the SDK, as not all the schema
   * changes in the RPC response will affect the caller, but the caller needs to
   * understand that the data may not match the TypeSrcript definitions.
   */
  constructor(
    public endpoint: string,
    public skipDataValidation: boolean = false
  ) {
    super();

    this.client = new JsonRpcClient(endpoint);
    // setup socket object so that it's defined in the constructor, but don't connect
    this.wsEndpoint = getWebsocketUrl(endpoint);
    const socketOptions = { reconnect_interval: 3000, autoconnect: false };
    this.wsClient = new WsRpcClient(this.wsEndpoint, socketOptions);
  }

  private setupSocket() {
    if(this.wsSetup) return;

    this.wsClient.on('open', () => {
      if(this.wsTimeout) {
        clearTimeout(this.wsTimeout);
        this.wsTimeout = null;
      }
      this.wsConnectionState = ConnectionState.Connected;
      // underlying websocket is private, but we need it
      // to access messages sent by the node
      (this.wsClient as any).socket
        .on('message', this.onSocketMessage.bind(this));
    });

    this.wsClient.on('close', () => {
      this.wsConnectionState = ConnectionState.NotConnected;
    });

    this.wsClient.on('error', console.error);
    this.wsSetup = true;
  }

  private async connect(): Promise<void> {
    if (this.wsConnectionState === ConnectionState.Connected)
      return Promise.resolve();

    this.setupSocket();
    this.wsClient.connect();
    this.wsConnectionState = ConnectionState.Connecting;

    return new Promise<void>((resolve, reject) => {
      this.wsTimeout = setTimeout(() => reject(new Error('timeout')), 15000) as any as number;
      this.wsClient.once('open', () => {
        this.refreshSubscriptions();
        resolve();
      });
      this.wsClient.once('error', (err) => {
        reject(err);
      });
    });
  }

  // called for every message received from the node over websocket
  private onSocketMessage(rawMessage: string): void {
    const msg: JsonRpcMethodMessage<object> = JSON.parse(rawMessage);

    const params = msg.params;
    if(msg.method === 'sui_subscribeEvent' && isSubscriptionEvent(params)) {
      // call any registered handler for the message's subscription
      const sub = this.activeSubscriptions.get(params.subscription);
      if (sub)
        sub.onMessage(params.result);
    }
  }

  /**
    call only upon reconnecting to a node over websocket.
    calling multiple times on the same connection will result
    in multiple message handlers firing each time
  */
  private async refreshSubscriptions() {
    if(this.activeSubscriptions.size === 0)
      return;

    try {
      let newSubs: Map<SubscriptionId, SubscriptionData> = new Map();

      let newSubsArr: (FilterSubHandler | null)[] = await Promise.all(
        Array.from(this.activeSubscriptions.values())
        .map(async sub => {
          const onMessage = sub.onMessage;
          const filter = sub.filter;
          if(!filter || !onMessage)
            return Promise.resolve(null);
          /**
            re-subscribe to the same filter & replace the subscription id.
            we skip calling sui_unsubscribeEvent for the old sub id, because:
              * we assume this is being called after a reconnection
              * the node keys subscriptions with a combo of connection id & subscription id
          */
          const id = await this.subscribeEvent(filter, onMessage);
          return { id, onMessage, filter };
        })
      );

      newSubsArr.forEach(entry => {
        if(entry === null) return;
        const filter = entry.filter;
        const onMessage = entry.onMessage;
        newSubs.set(entry.id, { filter, onMessage });
      });

      this.activeSubscriptions = newSubs;
    } catch (err) {
      throw new Error(`error refreshing event subscriptions: ${err}`);
    }
  }

  // Move info
  async getMoveFunctionArgTypes(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveFunctionArgTypes> {
    try {
      return await this.client.requestWithType(
        'sui_getMoveFunctionArgTypes',
        [objectId, moduleName, functionName],
        isSuiMoveFunctionArgTypes,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching Move function arg types with package object ID: ${objectId}, module name: ${moduleName}, function name: ${functionName}`
      );
    }
  }

  async getNormalizedMoveModulesByPackage(
    objectId: string
  ): Promise<SuiMoveNormalizedModules> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModulesByPackage',
        [objectId],
        isSuiMoveNormalizedModules,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(`Error fetching package: ${err} for package ${objectId}`);
    }
  }

  async getNormalizedMoveModule(
    objectId: string,
    moduleName: string
  ): Promise<SuiMoveNormalizedModule> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveModule',
        [objectId, moduleName],
        isSuiMoveNormalizedModule,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching module: ${err} for package ${objectId}, module ${moduleName}}`
      );
    }
  }

  async getNormalizedMoveFunction(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveNormalizedFunction> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveFunction',
        [objectId, moduleName, functionName],
        isSuiMoveNormalizedFunction,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching function: ${err} for package ${objectId}, module ${moduleName} and function ${functionName}}`
      );
    }
  }

  async getNormalizedMoveStruct(
    objectId: string,
    moduleName: string,
    structName: string
  ): Promise<SuiMoveNormalizedStruct> {
    try {
      return await this.client.requestWithType(
        'sui_getNormalizedMoveStruct',
        [objectId, moduleName, structName],
        isSuiMoveNormalizedStruct,
        this.skipDataValidation
      );
    } catch (err) {
      throw new Error(
        `Error fetching struct: ${err} for package ${objectId}, module ${moduleName} and struct ${structName}}`
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
    const requests = objectIds.map(id => ({
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

  async getTransactionsForObject(
    objectID: string
  ): Promise<GetTxnDigestsResponse> {
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
        isGetTxnDigestsResponse,
        this.skipDataValidation
      );
      return [...results[0], ...results[1]];
    } catch (err) {
      throw new Error(
        `Error getting transactions for object: ${err} for id ${objectID}`
      );
    }
  }

  async getTransactionsForAddress(
    addressID: string
  ): Promise<GetTxnDigestsResponse> {
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
        isGetTxnDigestsResponse,
        this.skipDataValidation
      );
      return [...results[0], ...results[1]];
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
    const requests = digests.map(d => ({
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

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId> {
    try {
      // lazily connect to websocket to avoid spamming node with connections
      if (this.wsConnectionState != ConnectionState.Connected)
        await this.connect();

      let subId = await this.wsClient.call(
        'sui_subscribeEvent',
        [filter],
        30000
      ) as SubscriptionId;

      this.activeSubscriptions.set(subId, { filter, onMessage });
      return subId;
    } catch (err) {
      throw new Error(
        `Error subscribing to event: ${err}, filter: ${JSON.stringify(filter)}`
      );
    }
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    try {
      if (this.wsConnectionState != ConnectionState.Connected)
        await this.connect();

      let removedOnNode = await this.wsClient.call(
        'sui_unsubscribeEvent',
        [id],
        30000
      ) as boolean;
      /**
        if the connection closes before unsubscribe is called,
        the remote node will remove us from its subscribers list without notification,
        leading to removedOnNode being false. but if we still had a record of it locally,
        we should still report that it was deleted successfully
      */
      return this.activeSubscriptions.delete(id) || removedOnNode;
    } catch (err) {
      throw new Error(
        `Error unsubscribing from event: ${err}, subscription: ${id}}`
      );
    }
  }
}
