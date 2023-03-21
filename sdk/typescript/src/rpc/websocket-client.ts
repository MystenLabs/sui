// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is } from 'superstruct';
import {
  SuiEventFilter,
  SubscriptionId,
  SubscriptionEvent,
  SuiEvent,
} from '../types';
import { Client as WsRpcClient } from 'rpc-websockets';

export const getWebsocketUrl = (httpUrl: string, port?: number): string => {
  const url = new URL(httpUrl);
  url.protocol = url.protocol.replace('http', 'ws');
  if (port) {
    url.port = port.toString();
  }
  return url.toString();
};

enum ConnectionState {
  NotConnected,
  Connecting,
  Connected,
}

type JsonRpcMethodMessage<T> = {
  jsonrpc: '2.0';
  method: string;
  params: T;
};

type FilterSubHandler = {
  id: SubscriptionId;
  onMessage: (event: SuiEvent) => void;
  filter: SuiEventFilter;
};

type SubscriptionData = {
  filter: SuiEventFilter;
  onMessage: (event: SuiEvent) => void;
};

type MinimumSubscriptionMessage = {
  subscription: SubscriptionId;
  result: object;
};

const isMinimumSubscriptionMessage = (
  msg: any,
): msg is MinimumSubscriptionMessage =>
  msg &&
  'subscription' in msg &&
  typeof msg['subscription'] === 'number' &&
  'result' in msg &&
  typeof msg['result'] === 'object';

/**
 * Configuration options for the websocket connection
 */
export type WebsocketClientOptions = {
  /**
   * Milliseconds before timing out while initially connecting
   */
  connectTimeout: number;
  /**
   * Milliseconds before timing out while calling an RPC method
   */
  callTimeout: number;
  /**
   * Milliseconds between attempts to connect
   */
  reconnectInterval: number;
  /**
   * Maximum number of times to try connecting before giving up
   */
  maxReconnects: number;
};

export const DEFAULT_CLIENT_OPTIONS: WebsocketClientOptions = {
  connectTimeout: 15000,
  callTimeout: 30000,
  reconnectInterval: 3000,
  maxReconnects: 5,
};

const SUBSCRIBE_EVENT_METHOD = 'sui_subscribeEvent';
const UNSUBSCRIBE_EVENT_METHOD = 'sui_unsubscribeEvent';

/**
 * Interface with a Sui node's websocket capabilities
 */
export class WebsocketClient {
  protected rpcClient: WsRpcClient;
  protected connectionState: ConnectionState = ConnectionState.NotConnected;
  protected connectionTimeout: number | null = null;
  protected isSetup: boolean = false;
  private connectionPromise: Promise<void> | null = null;

  protected eventSubscriptions: Map<SubscriptionId, SubscriptionData> =
    new Map();

  /**
   * @param endpoint Sui node endpoint to connect to (accepts websocket & http)
   * @param skipValidation If `true`, the rpc client will not check if the responses
   * from the RPC server conform to the schema defined in the TypeScript SDK
   * @param options Configuration options, such as timeouts & connection behavior
   */
  constructor(
    public endpoint: string,
    public skipValidation: boolean,
    public options: WebsocketClientOptions = DEFAULT_CLIENT_OPTIONS,
  ) {
    if (this.endpoint.startsWith('http'))
      this.endpoint = getWebsocketUrl(this.endpoint);

    this.rpcClient = new WsRpcClient(this.endpoint, {
      reconnect_interval: this.options.reconnectInterval,
      max_reconnects: this.options.maxReconnects,
      autoconnect: false,
    });
  }

  private setupSocket() {
    if (this.isSetup) return;

    this.rpcClient.on('open', () => {
      if (this.connectionTimeout) {
        clearTimeout(this.connectionTimeout);
        this.connectionTimeout = null;
      }
      this.connectionState = ConnectionState.Connected;
      // underlying websocket is private, but we need it
      // to access messages sent by the node
      (this.rpcClient as any).socket.on(
        'message',
        this.onSocketMessage.bind(this),
      );
    });

    this.rpcClient.on('close', () => {
      this.connectionState = ConnectionState.NotConnected;
    });

    this.rpcClient.on('error', console.error);
    this.isSetup = true;
  }

  // called for every message received from the node over websocket
  private onSocketMessage(rawMessage: string): void {
    const msg: JsonRpcMethodMessage<object> = JSON.parse(rawMessage);

    const params = msg.params;
    if (msg.method === SUBSCRIBE_EVENT_METHOD) {
      // even with validation off, we must ensure a few properties at minimum in a message
      if (this.skipValidation && isMinimumSubscriptionMessage(params)) {
        const sub = this.eventSubscriptions.get(params.subscription);
        if (sub)
          // cast to bypass type validation of 'result'
          (sub.onMessage as (m: any) => void)(params.result);
      } else if (is(params, SubscriptionEvent)) {
        // call any registered handler for the message's subscription
        const sub = this.eventSubscriptions.get(params.subscription);
        if (sub) sub.onMessage(params.result);
      }
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
        this.options.connectTimeout,
      ) as any as number;

      this.rpcClient.once('open', () => {
        this.refreshSubscriptions();
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
  private async refreshSubscriptions() {
    if (this.eventSubscriptions.size === 0) return;

    try {
      let newSubs: Map<SubscriptionId, SubscriptionData> = new Map();

      let newSubsArr: (FilterSubHandler | null)[] = await Promise.all(
        Array.from(this.eventSubscriptions.values()).map(async (sub) => {
          const onMessage = sub.onMessage;
          const filter = sub.filter;
          if (!filter || !onMessage) return Promise.resolve(null);
          /**
            re-subscribe to the same filter & replace the subscription id.
            we skip calling sui_unsubscribeEvent for the old sub id, because:
              * we assume this is being called after a reconnection
              * the node keys subscriptions with a combo of connection id & subscription id
          */
          const id = await this.subscribeEvent(filter, onMessage);
          return { id, onMessage, filter };
        }),
      );

      newSubsArr.forEach((entry) => {
        if (entry === null) return;
        const filter = entry.filter;
        const onMessage = entry.onMessage;
        newSubs.set(entry.id, { filter, onMessage });
      });

      this.eventSubscriptions = newSubs;
    } catch (err) {
      throw new Error(`error refreshing event subscriptions: ${err}`);
    }
  }

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEvent) => void,
  ): Promise<SubscriptionId> {
    try {
      // lazily connect to websocket to avoid spamming node with connections
      if (this.connectionState !== ConnectionState.Connected)
        await this.connect();

      let subId = (await this.rpcClient.call(
        SUBSCRIBE_EVENT_METHOD,
        [filter],
        this.options.callTimeout,
      )) as SubscriptionId;

      this.eventSubscriptions.set(subId, { filter, onMessage });
      return subId;
    } catch (err) {
      throw new Error(
        `Error subscribing to event: ${JSON.stringify(
          err,
          null,
          2,
        )}, filter: ${JSON.stringify(filter)}`,
      );
    }
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    try {
      if (this.connectionState !== ConnectionState.Connected)
        await this.connect();

      let removedOnNode = (await this.rpcClient.call(
        UNSUBSCRIBE_EVENT_METHOD,
        [id],
        this.options.callTimeout,
      )) as boolean;
      /**
        if the connection closes before unsubscribe is called,
        the remote node will remove us from its subscribers list without notification,
        leading to removedOnNode being false. but if we still had a record of it locally,
        we should still report that it was deleted successfully
      */
      return this.eventSubscriptions.delete(id) || removedOnNode;
    } catch (err) {
      throw new Error(
        `Error unsubscribing from event: ${err}, subscription: ${id}`,
      );
    }
  }
}
