// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiEventEnvelope, isSuiTransactionResponse } from '../types/index.guard';
import {
  SuiEventFilter,
  SuiEventEnvelope,
  SubscriptionId,
  SuiTransactionFilter,
  SuiTransactionResponse,
} from '../types';
import { Client as WsRpcClient} from 'rpc-websockets';
import { isAny } from '../providers/json-rpc-provider';


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

type FilterSubHandler<TFilter, TResponse> = {
  id: SubscriptionId,
  onMessage: (msg: TResponse) => void,
  filter: TFilter
};

type FilterCallbackPair<TFilter, TOnMessageInput> = {
  filter: TFilter,
  onMessage: (tx: TOnMessageInput) => void
}

type EventSubscriptionData = FilterCallbackPair<SuiEventFilter, SuiEventEnvelope>;
type TransactionSubscriptionData = FilterCallbackPair<SuiTransactionFilter, SuiTransactionResponse>;

type MinimumSubscriptionMessage = {
  subscription: SubscriptionId,
  result: object
}

const isMinimumSubscriptionMessage = (msg: any): msg is MinimumSubscriptionMessage =>
  msg
  && ('subscription' in msg && typeof msg['subscription'] === 'number')
  && ('result' in msg && typeof msg['result'] === 'object')

/**
 * Configuration options for the websocket connection
 */
 export type WebsocketClientOptions = {
  /**
   * Milliseconds before timing out while initially connecting
   */
  connectTimeout: number,
  /**
   * Milliseconds before timing out while calling an RPC method
   */
  callTimeout: number,
  /**
   * Milliseconds between attempts to connect
   */
  reconnectInterval: number,
  /**
   * Maximum number of times to try connecting before giving up
   */
  maxReconnects: number
}

export const DEFAULT_CLIENT_OPTIONS: WebsocketClientOptions = {
  connectTimeout: 15000,
  callTimeout: 30000,
  reconnectInterval: 3000,
  maxReconnects: 5
}

const SUBSCRIBE_EVENT_METHOD = 'sui_subscribeEvent';
const UNSUBSCRIBE_EVENT_METHOD = 'sui_unsubscribeEvent';
const SUBSCRIBE_TRANSACTION_METHOD = 'sui_subscribeTransaction';
const UNSUBSCRIBE_TRANSACTION_METHOD = 'sui_unsubscribeTransaction';

/**
 * Interface with a Sui node's websocket capabilities
 */
export class WebsocketClient {
  protected rpcClient: WsRpcClient;
  protected connectionState: ConnectionState = ConnectionState.NotConnected;
  protected connectionTimeout: number | null = null;
  protected isSetup: boolean = false;
  private connectionPromise: Promise<void> | null = null;

  protected eventSubscriptions: Map<SubscriptionId, EventSubscriptionData> = new Map();
  protected txSubscriptions: Map<SubscriptionId, TransactionSubscriptionData> = new Map();

  /**
   * @param endpoint Sui node endpoint to connect to (accepts websocket & http)
   * @param skipValidation If `true`, the rpc client will not check if the responses
   * from the RPC server conform to the schema defined in the TypeScript SDK
   * @param options Configuration options, such as timeouts & connection behavior
   */
  constructor(
    public endpoint: string,
    public skipValidation: boolean,
    public options: WebsocketClientOptions = DEFAULT_CLIENT_OPTIONS
  ) {
    if (this.endpoint.startsWith('http'))
      this.endpoint = getWebsocketUrl(this.endpoint);

    this.rpcClient = new WsRpcClient(this.endpoint, {
      reconnect_interval: this.options.reconnectInterval,
      max_reconnects: this.options.maxReconnects,
      autoconnect: false
    });
  }

  private setupSocket() {
    if(this.isSetup) return;

    this.rpcClient.on('open', () => {
      if(this.connectionTimeout) {
        clearTimeout(this.connectionTimeout);
        this.connectionTimeout = null;
      }
      this.connectionState = ConnectionState.Connected;
      // underlying websocket is private, but we need it
      // to access messages sent by the node
      (this.rpcClient as any).socket
        .on('message', this.onSocketMessage.bind(this));
    });

    this.rpcClient.on('close', () => {
      this.connectionState = ConnectionState.NotConnected;
    });

    this.rpcClient.on('error', console.error);
    this.isSetup = true;
  }

  // find the associated callback for a subscription response & run it
  private execOnMessage<TFilter, TOnMessageInput, TParams extends MinimumSubscriptionMessage>(
    map: Map<SubscriptionId, FilterCallbackPair<TFilter, TOnMessageInput>>,
    params: TParams,
    isMessageInput: (o: any) => o is TOnMessageInput
  ) {
    if (isMessageInput(params.result)) {
      const sub = map.get(params.subscription);
      if (sub)
        // call any registered handler for the message's subscription
        sub.onMessage(params.result);
    }
  }

  // called for every message received from the node over websocket
  private onSocketMessage(rawMessage: string): void {
    const msg: JsonRpcMethodMessage<object> = JSON.parse(rawMessage);
    if (!isMinimumSubscriptionMessage(msg.params))
      return;

    switch (msg.method) {
      case SUBSCRIBE_TRANSACTION_METHOD:
          const txTypeGuard = this.skipValidation ? isAny : isSuiTransactionResponse;
          this.execOnMessage(this.txSubscriptions, msg.params, txTypeGuard);
        break;
      case SUBSCRIBE_EVENT_METHOD:
          const eventTypeGuard = this.skipValidation ? isAny : isSuiEventEnvelope;
          this.execOnMessage(this.eventSubscriptions, msg.params, eventTypeGuard);
        break;
    }
  }

  private async connect(): Promise<void> {
    // if the last attempt to connect hasn't finished, wait on it
    if (this.connectionPromise) return this.connectionPromise;
    if (this.connectionState === ConnectionState.Connected)
      return Promise.resolve();

    this.setupSocket();
    this.rpcClient.connect();
    this.connectionState = ConnectionState.Connecting;

    this.connectionPromise = new Promise<void>((resolve, reject) => {
      this.connectionTimeout = setTimeout(
        () => reject(new Error('timeout')),
        this.options.connectTimeout
      ) as any as number;

      this.rpcClient.once('open', () => {
        this.refreshAllSubscriptions();
        this.connectionPromise = null;
        resolve();
      });
      this.rpcClient.once('error', (err) => {
        this.connectionPromise = null;
        reject(err);
      });
    });
    return this.connectionPromise;
  }

  /**
    call only upon reconnecting to a node over websocket.
    calling multiple times on the same connection will result
    in multiple message handlers firing each time
  */
  private async refreshSubscriptions<TFilter, TResponse>(
    subMap: Map<SubscriptionId, FilterCallbackPair<TFilter, TResponse>>,
    subscribeMethod: (f: TFilter, onMsg: (m: TResponse) => void) => Promise<SubscriptionId>
  ) {
    if (subMap.size === 0)
      return subMap;

    try {
      let newSubs: Map<SubscriptionId, FilterCallbackPair<TFilter, TResponse>> = new Map();

      let newSubsArr: (FilterSubHandler<TFilter, TResponse> | null)[] = await Promise.all(
        Array.from(subMap.values())
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
          const id = await subscribeMethod(filter, onMessage);
          return { id, onMessage, filter };
        })
      );

      newSubsArr.forEach(entry => {
        if(entry === null) return;
        const filter = entry.filter;
        const onMessage = entry.onMessage;
        newSubs.set(entry.id, { filter, onMessage });
      });

      return newSubs;
    } catch (err) {
      throw new Error(`error refreshing subscriptions: ${err}`);
    }
  }

  private async refreshAllSubscriptions() {
    this.eventSubscriptions = await this.refreshSubscriptions(this.eventSubscriptions, this.subscribeEvent.bind(this));
    this.txSubscriptions = await this.refreshSubscriptions(this.txSubscriptions, this.subscribeTransaction.bind(this));
  }

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId> {
    try {
      // lazily connect to websocket to avoid spamming node with connections
      if (this.connectionState != ConnectionState.Connected)
        await this.connect();

      let subId = await this.rpcClient.call(
        SUBSCRIBE_EVENT_METHOD,
        [filter],
        this.options.callTimeout
      ) as SubscriptionId;

      this.eventSubscriptions.set(subId, { filter, onMessage });
      return subId;
    } catch (err) {
      throw new Error(
        `Error subscribing to event: ${err}, filter: ${JSON.stringify(filter)}`
      );
    }
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    try {
      if (this.connectionState != ConnectionState.Connected)
        await this.connect();

      let removedOnNode = await this.rpcClient.call(
        UNSUBSCRIBE_EVENT_METHOD,
        [id],
        this.options.callTimeout
      ) as boolean;
      /**
        if the connection closes before unsubscribe is called,
        the remote node will remove us from its subscribers list without notification,
        leading to removedOnNode being false. but if we still had a record of it locally,
        we should still report that it was deleted successfully
      */
      return this.eventSubscriptions.delete(id) || removedOnNode;
    } catch (err) {
      throw new Error(
        `Error unsubscribing from event: ${err}, subscription: ${id}}`
      );
    }
  }

  async subscribeTransaction(
    filter: SuiTransactionFilter,
    onMessage: (tx: SuiTransactionResponse) => void
  ): Promise<SubscriptionId> {
    try {
      if (this.connectionState != ConnectionState.Connected)
        await this.connect();

      let subId = await this.rpcClient.call(
        SUBSCRIBE_TRANSACTION_METHOD,
        [filter],
        this.options.callTimeout
      ) as SubscriptionId;

      this.txSubscriptions.set(subId, { filter, onMessage });
      return subId;
    } catch (err) {
      throw new Error(
        `Error subscribing to transactions: ${JSON.stringify(err)}`
      );
    }
  }

  async unsubscribeTransaction(id: SubscriptionId): Promise<boolean> {
    try {
      if (this.connectionState != ConnectionState.Connected)
        await this.connect();

      let removedOnNode = await this.rpcClient.call(
        UNSUBSCRIBE_TRANSACTION_METHOD,
        [id],
        this.options.callTimeout
      ) as boolean;

      return this.txSubscriptions.delete(id) || removedOnNode;
    } catch (err) {
      throw new Error(
        `Error unsubscribing from transactions: ${JSON.stringify(err)}`
      );
    }
  }
}