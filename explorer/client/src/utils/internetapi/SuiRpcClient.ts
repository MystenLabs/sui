// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { tryGetRpcSetting } from './rpcSetting';
import { JsonRpcProvider } from 'sui.js';


export type AddressBytes = number[];
export type Signature = number[];

export type AddressOwner = { AddressOwner: AddressBytes };
type ObjectOwner = { ObjectOwner: AddressBytes };

export type AnyVec = { vec: any[] };
type BoolString = 'true' | 'false';

export type JsonBytes = { bytes: number[] };
export type MoveVec<T extends object | string> = { vec: T[] };

export interface ObjectInfoResponse<T> {
    owner: string;
    version: string;
    id: string;
    readonly: BoolString;
    objType: string;
    data: SuiObject<T>;
}

export interface SuiObject<T> {
    contents: T;
    owner: ObjectOwner | AddressOwner;
    tx_digest: number[];
}

export interface ObjectSummary {
    id: string;
    object_digest: string;
    type: string;
    version: string;
}

export interface ObjectEffectsSummary {
    created_objects?: ObjectSummary[];
    mutated_objects?: ObjectSummary[];
    deleted_objects?: ObjectSummary[];
}

export interface CallTransactionResponse {
    function: string;
    gas_budget: number;
    module: string;
    object_arguments: any[];
    package: any[];
    pure_arguments: number[][];
    shared_object_arguments: any[];
    type_arguments: any[];
}

export interface TransactionKind {
    Call?: CallTransactionResponse;
}

export interface TransactionData {
    gas_payment: any[];
    kind: TransactionKind;
    sender: AddressBytes;
}

export interface Transaction {
    data: TransactionData;
    signature: Signature;
}

export interface Certificate {
    signatures: Signature[][];
    transaction: Transaction;
}

export interface MoveCallResponse {
    gasUsed: number;
    objectEffectsSummary: ObjectEffectsSummary;
    certificate: Certificate;
}

export interface Addresses {
    addresses: string[];
}

export interface AddressObjectsResponse {
    id?: string; // TODO - can we remove this ?
    objects: AddressObjectSummary[];
}

// TODO - this format is inconsistent with other object summaries (camelCase vs snake_case, lack of type field),
// which needs to be changed in the backend RPC
// TODO - also needs stronger types for fields
export interface AddressObjectSummary {
    objectId: string;
    version: string;
    objectDigest: string;
}

const rpcUrl = tryGetRpcSetting() ?? 'https://demo-rpc.sui.io';

export const DefaultRpcClient = new JsonRpcProvider(rpcUrl);
