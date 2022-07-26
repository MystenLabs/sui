// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * This module handles RPC interaction by providing a client.
 * And a set of endpoints to talk to.
 *
 * @module rpc
 */

import { SuiObjectRef } from "./types";

/**
 * Result of a `sui_getObject` RPC call.
 * Extends `SuiObjectRef` and can be used as one.
 */
export interface OwnedObjectResponse extends SuiObjectRef {
  objectId: string;
  version: number;
  digest: string;
  type: string;
  owner: { AddressOwner: string };
  previousTransaction: string;
}

/**
 * Object where keys are objectId's and values are SuiObjectRef.
 */
export type RefsById = { [key: string]: SuiObjectRef };

/**
 * Type that unifies transaction responses from MoveCallTx
 * and PublishTx. Tends to simplify adoption by reusing the
 * same set of fields.
 * @example
 * let { created, mutated, raw } = await sendTx(...);
 */
export type TxResponse = {
  /**
   * Status is only returned for @MoveCallTx.
   * Holds the value of kind:
   * @example
   * { "status": "success" }
   * // OR
   * { "status": "error", "...": "..." }
   */
  status: { status: string };
  /**
   * Lists mutated objects. Only available for @MoveCallTx.
   * Publish Transactions don't mutate objects (except gas).
   */
  mutated: RefsById;
  /**
   * List of objects that have been wrapped during the transaction.
   * Only available for the @MoveCallTx
   */
  wrapped: RefsById;
  /**
   * List of objects that have been unwrapped during the transaction.
   */
  unwrapped: RefsById;
  /**
   * Lists created objects indexed by objectId. Available for
   * both @MoveCallTx and @PublishTx
   */
  created: RefsById;
  /**
   * Lists created objects grouped by their types.
   * Only @PublishTx
   */
  createdByType: { [key: string]: SuiObjectRef[] };
  /**
   * Lists deleted objects indexed by objectid. Only available in
   * @MoveCallTx
   */
  deleted: { [key: string]: SuiObjectRef };
  /**
   * List of events triggered when executing a transaction. Not
   * available in @PublishTx - @TODO
   */
  events: { [key: string]: string };
  /**
   * Package created when executing @PublishTx transaction. Can not
   * exist in @MoveCallTx
   */
  package?: SuiObjectRef;
  /**
   * Updated SuiObjectRef for the gas object that was used when executing
   * the transaction.
   */
  gas: SuiObjectRef;
  /**
   * Additional property which indicates the amount of gas used when calling
   * @MoveCallTx transaction type.
   */
  gasUsed?: {
    computationCost: number;
    storageCost: number;
    storageRebate: number;
  };
  /**
   * Contains the raw (unprocessed) data returned by the gateway.
   */
  raw: object;
};

/**
 * Base client for RPC calls.
 * Sends txs to the Gateway and queries data from the FullNode.
 */
export class RpcClient {
  /**
   * Request callback for the FullNode.
   */
  protected req: (method: string, params: any[]) => any;

  /**
   * Request callback for the Gateway (to send TXs).
   */
  protected reqTx: (method: string, params: any[]) => any;

  constructor(gatewayUrl: string, fullNodeUrl: string) {
    this.reqTx = setupClient(gatewayUrl);
    this.req = setupClient(fullNodeUrl);
  }

  /**
   * Fetch an object by ID.
   * @param id ID of the object to fetch
   */
  public async fetchObj(id: string): Promise<any> {
    let res = await this.req("sui_getObject", [id]);
    return res;
  }

  /**
   * Fetch Object reference.
   * @param id ID of the object to fetch
   */
  public async fetchObjRef(id: string): Promise<SuiObjectRef> {
    return this.fetchObj(id).then(
      (o) => (o.details && o.details.reference) || null
    );
  }

  /**
   * Get objects owned by address.
   * @param address Address of the account to fetch objects for
   */
  public async myObjects(address: string): Promise<OwnedObjectResponse[]> {
    return this.req("sui_getObjectsOwnedByAddress", [address]);
  }

  /**
   * Get objects owned by address and group them by type.
   * @param address Address of the account to fetch objects for
   */
  public async myObjectsByType(
    address: string
  ): Promise<{ [key: string]: OwnedObjectResponse[] }> {
    let objects = await this.myObjects(address);
    return objects.reduce((acc: any, val: OwnedObjectResponse) => {
      let type = val.type!;
      acc[type] ? acc[type].push(val) : (acc[type] = [val]);
      return acc;
    }, {});
  }

  /**
   * Send a signed transaction to the Gateway.
   */
  public async sendTx(args: any[]): Promise<TxResponse> {
    let res = await this.reqTx("sui_executeTransaction", args);
    if (res.error) {
      return Promise.reject(res);
    }

    // Process MoveCallTx and reach uniformity with the TxResponse type.
    if ("EffectResponse" in res) {
      let processed = res.EffectResponse.effects;

      let created = groupRefs(processed.created || []);
      let mutated = groupRefs(processed.mutated || []);
      let wrapped = groupRefs(processed.wrapped || []);
      let unwrapped = groupRefs(processed.unwrapped || []);
      let deleted = groupRefs(processed.deleted || []);

      let events = (processed.events || [])
        .filter((obj: object) => "moveEvent" in obj)
        .reduce(
          (acc: object, { moveEvent: evt }: any) =>
            Object.assign(acc, {
              [evt.type]: evt.bcs,
            }),
          {}
        );

      return {
        status: processed.status,
        mutated,
        wrapped,
        unwrapped,
        created,
        createdByType: {},
        deleted,
        events,
        gas: processed.gasObject.reference,
        gasUsed: processed.gasUsed,
        raw: res.EffectResponse,
      };
    }

    // Process publish transaction and reach uniformity with the TxResponse type.
    if ("PublishResponse" in res) {
      res = res.PublishResponse;

      let created = groupRefs(res.createdObjects || []);
      let createdByType = (res.createdObjects || []).reduce(
        (acc: { [key: string]: any }, obj: any) =>
          Object.assign(acc, {
            [obj.data.type]:
              obj.data.type in acc
                ? acc[obj.data.type].concat(obj.reference)
                : [obj.reference],
          }),
        {}
      );

      return {
        status: res.status,
        created,
        createdByType,
        wrapped: {},
        unwrapped: {},
        mutated: {},
        deleted: {},
        events: {},
        package: res.package,
        gas: res.updatedGas.reference,
        raw: res,
      };
    }

    throw new Error("Unknown transaction response");
  }
}

/**
 * Creates a callback function to query some endpoint.
 *
 * Both Gateway and FullNode use the JSON RPC standard, so the client
 * code looks the same for both and can be easily implemented without
 * unnecessary dependencies.
 */
function setupClient(origin: string) {
  let counter = 0;

  return async function request(
    method: string,
    params: object | Array<any>
  ): Promise<any> {
    let body = {
      jsonrpc: "2.0",
      method,
      id: counter++,
      params: params.constructor === Array ? params : Object.values(params),
    };

    let res = await fetch(origin, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });

    let json = await res.json();
    let result = json.result ? json.result : json;

    return res.status === 200 ? result : Promise.reject(json);
  };
}

/**
 * Helper function to simplify references parsing.
 */
function groupRefs(
  array: { reference: SuiObjectRef }[] | SuiObjectRef[]
): RefsById {
  // @ts-ignore
  return array.reduce(
    (acc: any, obj: any) =>
      Object.assign(
        acc,
        obj.reference
          ? { [obj.reference.objectId]: obj.reference }
          : { [obj.objectId]: obj }
      ),
    {}
  );
}
