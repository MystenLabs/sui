// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SuiObjectRef,
  MoveCallTx,
  PublishTx,
  TransactionData,
  registerTypes,
} from "./types";
import { RpcClient, TxResponse } from "./rpc";
import { bcs as BCS, toB64 } from "@mysten/bcs";
import { sha3_256 } from "js-sha3";
import * as nacl from "tweetnacl";

// For commonjs scenario need to figure out how to do it differently
const bcs = registerTypes(BCS);
export { bcs };

/**
 * `nacl` keypair type
 */
export type KeyPair = { publicKey: Uint8Array; secretKey: Uint8Array };

/**
 * Represents a signed transaction which can be sent directly to
 * the Gateway service to the `sui_executeTransaction` method.
 */
export type SignedTx = [string, string, string];

/**
 * Handy data manager and a tx sender. Collects all the data ever received
 * in transaction responses, therefore allowing faster usage. Due to most of
 * the objects being shared or owned by the current user, this way of collecting
 * data can be considered safe enough. Especially if SuiClient is run by SuiWallet.
 */
export class SuiClient {
  /**
   * Handy initializer for local development.
   * @param gasRef Optional gas setting
   */
  static local(gasRef: SuiObjectRef | null = null): SuiClient {
    return new this("http://127.0.0.1:5001", "http://0.0.0.0:9000", gasRef);
  }

  /**
   * Handy initializer for devnet.
   * @param gasRef Optional gas setting
   */
  static devnet(gasRef: SuiObjectRef | null = null): SuiClient {
    return new this(
      "https://gateway.devnet.sui.io",
      "https://fullnode.devnet.sui.io",
      gasRef
    );
  }

  /**
   * A list of objects which are being tracked. All records
   * from EffectsReponse | PublishResponse are added here.
   *
   * Whenever an object is fetched or updated within the transaction,
   * its record gets updated.
   */
  protected objects: Map<string, SuiObjectRef> = new Map();

  /**
   * An RPC client to use.
   */
  protected rpc: RpcClient;

  constructor(
    /**
     * A URL to send transactions to.
     * To be removed once Full Node starts accepting txs.
     */
    gatewayUrl: string,
    /**
     * A URL to query data from.
     */
    fullNodeUrl: string,
    /**
     * A gas to use when sending transactions.
     * Version is automatically upated from tx response.
     */
    protected gasRef: SuiObjectRef | null = null
  ) {
    this.rpc = new RpcClient(gatewayUrl, fullNodeUrl);
  }

  /**
   * Fetch a local ref if it exists, return null otherwise
   * @param id ID of the object
   * @param force Whether to ignore cached version
   */
  public async ref(id: string, force: boolean = false): Promise<SuiObjectRef> {
    if (force) {
      return this.fetchObject(id);
    }

    return this.objects.has(id)
      ? Promise.resolve(this.objects.get(id)!)
      : this.fetchObject(id);
  }

  /**
   * Get owned objects' references grouped by a type.
   * @param address Address of an account to fetch objects for
   * @returns Object where keys are types and values are arrays of references
   */
  public async myObjectsByType(
    address: string
  ): Promise<{ [key: string]: SuiObjectRef[] }> {
    let objects = await this.rpc.myObjects(address);
    let byType: { [key: string]: SuiObjectRef[] } = {};
    for (let obj of objects) {
      obj.type in byType
        ? byType[obj.type].push(obj)
        : (byType[obj.type] = [obj]);

      this.objects.set(obj.objectId, {
        objectId: obj.objectId,
        version: obj.version,
        digest: obj.digest,
      });
    }

    return byType;
  }

  /**
   * Fetch an object from the network
   * @param id ID of the object
   */
  public async fetchObject(id: string): Promise<SuiObjectRef> {
    let ref = await this.rpc.fetchObjRef(id);
    this.objects.set(ref.objectId, ref);
    return ref;
  }

  /**
   * Balance of the current gas object used
   */
  public async gasBalance(): Promise<number> {
    const gas = this.gasOrFail();

    return this.rpc.fetchObj(gas.objectId).then((res) => {
      if (res.status === "Exists") {
        return res.details.data.fields.balance;
      } else {
        throw new Error(
          `Gas Object not found: ${gas.objectId}! \n${JSON.stringify(res)}`
        );
      }
    });
  }

  /**
   * Update gas object used for transactions; if it exists - use, if not -
   * query it from the network.
   * @param gasId ID of the Gas object to use
   */
  public async setGas(gasId: string): Promise<void> {
    if (this.objects.has(gasId)) {
      this.gasRef = this.objects.get(gasId)!;
    } else {
      this.gasRef = await this.rpc.fetchObjRef(gasId);
    }
  }

  /**
   * Send the transaction to the network and update local versions.
   * Currently only supports `MoveCallTx` and a `PublishTx`
   */
  public async send(
    keypair: KeyPair,
    tx: MoveCallTx | PublishTx,
    gasBudget = 5000,
    bcsBufferSize = 4096
  ): Promise<TxResponse> {
    let preparedTx = this.signTx(
      keypair,
      {
        kind: { Single: tx },
        gasPayment: this.gasOrFail(),
        gasPrice: 1,
        gasBudget,
      },
      bcsBufferSize
    );

    let result = await this.rpc.sendTx(preparedTx);

    result.mutated && this.updateLocalStorage(result.mutated);
    result.created && this.updateLocalStorage(result.created);
    result.wrapped && this.updateLocalStorage(result.wrapped);
    result.unwrapped && this.updateLocalStorage(result.unwrapped);
    result.deleted &&
      Object.keys(result.deleted).forEach((key) => this.objects.delete(key));

    this.gasRef = result.gas;
    return result;
  }

  /**
   * Update locally stored references of all mutated / created objects
   */
  updateLocalStorage(objects: { [key: string]: SuiObjectRef }) {
    for (let objId of Object.keys(objects)) {
      this.objects.set(objId, objects[objId]);
    }
  }

  /**
   * Check whether gas is set for the client.
   * Fails if not.
   *
   * @throws {Error} If gas object is not set.
   */
  gasOrFail(): SuiObjectRef {
    if (this.gasRef === null) {
      throw new Error(
        `To send transactions, please provide gas with sui.setGas(id) method`
      );
    }

    return this.gasRef;
  }

  /**
   * Sign the transaction with the given Ed25519Keypair instance.
   *
   * Steps:
   * - Fill in the `TransactionData.sender` field at the moment of signing;
   * - Add the TypeTag to the BcsBytes;
   * - Sign the resulting Uint8Array and get a signature.
   *
   * @param {Ed25519Keypair} keypair
   * @param {TransactionData} tx
   * @param {Number} size A size of a buffer in bytes
   * @returns {SignedTx} Transaction ready for sending
   */
  public signTx(
    pair: KeyPair,
    tx: TransactionData,
    size: number = 2048
  ): SignedTx {
    // Set TransactionData.sender field as PublicKey is known at the moment of signing.
    tx.sender = getAddress(pair.publicKey);

    let dataBytes = bcs.ser("TransactionData", tx, size).toBytes();
    let typeTag = Array.from("TransactionData::").map((e) => e.charCodeAt(0));
    let toSign = new Uint8Array(typeTag.length + dataBytes.length);

    toSign.set(typeTag);
    toSign.set(dataBytes, typeTag.length);

    return [
      toB64(toSign),
      toB64(nacl.sign.detached(toSign, pair.secretKey)),
      toB64(pair.publicKey),
    ];
  }
}

/**
 * Get the Sui address from a keypair using a `sha3_256` hash substring.
 */
export function getAddress(publicKey: Uint8Array): string {
  return sha3_256(publicKey).slice(0, 40);
}
