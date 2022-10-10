// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignatureScheme } from '../cryptography/publickey';
import { isSuiObjectRef } from '../types/index.guard';
import {
  GetObjectDataResponse,
  SuiObjectInfo,
  SuiTransactionResponse,
  SuiObjectRef,
  getObjectReference,
  TransactionEffects,
  normalizeSuiObjectId,
  ExecuteTransactionRequestType,
  SuiExecuteTransactionResponse,
  getTransactionEffects,
} from '../types';
import { JsonRpcProvider } from './json-rpc-provider';

export class JsonRpcProviderWithCache extends JsonRpcProvider {
  /**
   * A list of object references which are being tracked.
   *
   * Whenever an object is fetched or updated within the transaction,
   * its record gets updated.
   */
  private objectRefs: Map<string, SuiObjectRef> = new Map();

  // Objects
  async getObjectsOwnedByAddress(address: string): Promise<SuiObjectInfo[]> {
    const resp = await super.getObjectsOwnedByAddress(address);
    resp.forEach((r) => this.updateObjectRefCache(r));
    return resp;
  }

  async getObjectsOwnedByObject(objectId: string): Promise<SuiObjectInfo[]> {
    const resp = await super.getObjectsOwnedByObject(objectId);
    resp.forEach((r) => this.updateObjectRefCache(r));
    return resp;
  }

  async getObject(objectId: string): Promise<GetObjectDataResponse> {
    const resp = await super.getObject(objectId);
    this.updateObjectRefCache(resp);
    return resp;
  }

  async getObjectRef(
    objectId: string,
    skipCache = false
  ): Promise<SuiObjectRef | undefined> {
    const normalizedId = normalizeSuiObjectId(objectId);
    if (!skipCache && this.objectRefs.has(normalizedId)) {
      return this.objectRefs.get(normalizedId);
    }

    const ref = await super.getObjectRef(objectId);
    this.updateObjectRefCache(ref);
    return ref;
  }

  async getObjectBatch(objectIds: string[]): Promise<GetObjectDataResponse[]> {
    const resp = await super.getObjectBatch(objectIds);
    resp.forEach((r) => this.updateObjectRefCache(r));
    return resp;
  }

  // Transactions
  async executeTransaction(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string
  ): Promise<SuiTransactionResponse> {
    const resp = await super.executeTransaction(
      txnBytes,
      signatureScheme,
      signature,
      pubkey
    );

    this.updateObjectRefCacheFromTransactionEffects(resp.effects);
    return resp;
  }

  async executeTransactionWithRequestType(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string,
    requestType: ExecuteTransactionRequestType = 'WaitForEffectsCert'
  ): Promise<SuiExecuteTransactionResponse> {
    if (requestType !== 'WaitForEffectsCert') {
      console.warn(
        `It's not recommended to use JsonRpcProviderWithCache with the request ` +
          `type other than 'WaitForEffectsCert' for executeTransactionWithRequestType. Using ` +
          `the '${requestType}' may result in stale cache and a failure in subsequent transactions.`
      );
    }
    const resp = await super.executeTransactionWithRequestType(
      txnBytes,
      signatureScheme,
      signature,
      pubkey,
      requestType
    );
    const effects = getTransactionEffects(resp);
    if (effects != null) {
      this.updateObjectRefCacheFromTransactionEffects(effects);
    }
    return resp;
  }

  private updateObjectRefCache(
    newData: GetObjectDataResponse | SuiObjectRef | undefined
  ) {
    if (newData == null) {
      return;
    }
    const ref = isSuiObjectRef(newData) ? newData : getObjectReference(newData);
    if (ref != null) {
      this.objectRefs.set(ref.objectId, ref);
    }
  }

  private updateObjectRefCacheFromTransactionEffects(
    effects: TransactionEffects
  ) {
    effects.created?.forEach((r) => this.updateObjectRefCache(r.reference));
    effects.mutated?.forEach((r) => this.updateObjectRefCache(r.reference));
    effects.unwrapped?.forEach((r) => this.updateObjectRefCache(r.reference));
    effects.wrapped?.forEach((r) => this.updateObjectRefCache(r));
    effects.deleted?.forEach((r) => this.objectRefs.delete(r.objectId));
  }
}
