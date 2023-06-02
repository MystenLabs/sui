// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SubscriptionId } from '../types';
import {
  RequestManager,
  Client,
  WebSocketTransport,
} from '@open-rpc/client-js';

export const getWebsocketUrl = (httpUrl: string, port?: number): string => {
  const url = new URL(httpUrl);
  url.protocol = url.protocol.replace('http', 'ws');
  if (port) {
    url.port = port.toString();
  }
  return url.toString();
};

type NotificationMessageParams = {
  subscription: SubscriptionId;
  result: object;
};

type SubscriptionRequest<T = any> = {
  id?: number;
  method: string;
  unsubscribe: string;
  params: any[];
  onMessage: (event: T) => void;
};

/**
 * Configuration options for the websocket connection
 */
export type WebsocketClientOptions = {
  /**
   * Milliseconds before timing out while calling an RPC method
   */
  callTimeout: number;
  /**
   * Milliseconds between attempts to connect
   */
  reconnectTimeout: number;
  /**
   * Maximum number of times to try connecting before giving up
   */
  maxReconnects: number;
};

export const DEFAULT_CLIENT_OPTIONS: WebsocketClientOptions = {
  callTimeout: 30000,
  reconnectTimeout: 3000,
  maxReconnects: 5,
};

export class WebsocketClient {
  #client: Client | null;
  #subscriptions: Map<SubscriptionId, SubscriptionRequest & { id: number }>;
  #disconnects: number;

  constructor(
    public endpoint: string,
    public options: WebsocketClientOptions = DEFAULT_CLIENT_OPTIONS,
  ) {
    if (this.endpoint.startsWith('http')) {
      this.endpoint = getWebsocketUrl(this.endpoint);
    }

    this.#client = null;
    this.#subscriptions = new Map();
    this.#disconnects = 0;
  }

  #setupClient() {
    if (this.#client) {
      return this.#client;
    }

    const transport = new WebSocketTransport(this.endpoint);
    const requestManager = new RequestManager([transport]);
    this.#client = new Client(requestManager);

    transport.connection.addEventListener('open', () => {
      this.#disconnects = 0;
    });

    transport.connection.addEventListener('close', () => {
      this.#disconnects++;
      if (this.#disconnects <= this.options.maxReconnects) {
        setTimeout(() => {
          this.#reconnect();
        }, this.options.reconnectTimeout);
      }
    });

    this.#client.onNotification((data) => {
      const params = data.params as NotificationMessageParams;

      this.#subscriptions.forEach((subscription) => {
        if (
          subscription.method === data.method &&
          params.subscription === subscription.id
        ) {
          subscription.onMessage(params.result);
        }
      });
    });

    return this.#client;
  }

  #reconnect() {
    this.#client?.close();
    this.#client = null;

    this.#subscriptions.forEach((subscription) => this.request(subscription));
  }

  async request<T>(input: SubscriptionRequest<T>) {
    const client = this.#setupClient();
    const id = await client.request(
      { method: input.method, params: input.params },
      this.options.callTimeout,
    );

    // If an input ID is provided, this is a reconnect and we need to use that ID instead:
    this.#subscriptions.set(input.id || id, {
      ...input,
      // Always set the latest actual subscription ID:
      id,
    });

    return async () => {
      const client = this.#setupClient();
      // NOTE: Due to reconnects, the inner subscription ID could have actually changed:
      const subscription = this.#subscriptions.get(id);
      if (!subscription) return false;

      this.#subscriptions.delete(id);

      return client.request(
        { method: input.unsubscribe, params: [subscription.id] },
        this.options.callTimeout,
      );
    };
  }
}
