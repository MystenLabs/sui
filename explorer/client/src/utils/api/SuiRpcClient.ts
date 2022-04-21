// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { tryGetRpcSetting } from './rpcSetting';

export class SuiRpcClient {
    public readonly host: string;

    readonly moveCallUrl: string;
    readonly addressesUrl: string;

    // TODO - url type for host
    public constructor(host: string) {
        this.host = host;
        this.moveCallUrl = `${host}/wallet/call`;
        this.addressesUrl = `${host}/addresses`;
    }

    public getAddresses = async (): Promise<Addresses> =>
        this.fetchJson(this.addressesUrl);

    public getAddressObjects = async (address: AddressHexStr) => {
        const url = `${this.host}/objects?address=${address}`;
        return this.fetchJson(url);
    };

    public async getObjectInfo(id: string): Promise<ObjectInfoResponse> {
        const url = `${this.host}/object_info?objectId=${id}`;
        return this.fetchJson(url);
    }

    // TODO - more detailed type for input
    public async moveCall<TIn extends object | any[]>(
        input: TIn
    ): Promise<MoveCallResponse> {
        return this.postJson(this.moveCallUrl, input);
    }

    async fetchJson(url: string): Promise<any> {
        let response = await fetch(url, { mode: 'cors' });
        switch (response.status) {
            case 200:
                return response.json();
            case 424:
                throw new Error(
                    '424 response status - likely requesting missing data!'
                );
            default:
                throw new Error(
                    `unhandled HTTP response code: ${response.status}`
                );
        }
    }

    async postJson(url: string, body: object): Promise<any> {
        const response = await fetch(url, {
            mode: 'cors',
            method: 'POST',
            body: JSON.stringify(body),
            headers: { 'Content-Type': 'application/json' },
        });
        switch (response.status) {
            case 200:
                return response.json();
            default:
                throw new Error(
                    `non-200 response to POST ${this.moveCallUrl}: ${response.status}`
                );
        }
    }
}

export type AddressBytes = number[];
export type Signature = number[];

type AddressHexStr = string;

export type AddressOwner = { AddressOwner: AddressBytes };
type ObjectOwner = { ObjectOwner: AddressBytes };

export type AnyVec = { vec: any[] };
type BoolString = 'true' | 'false';

export type JsonBytes = { bytes: number[] };
export type MoveVec<T extends object | string> = { vec: T[] };

interface ContentsDefault {
    [key: string]: any;
}

export interface ObjectInfoResponse<T = ContentsDefault> {
    owner: string;
    version: string;
    id: string;
    readonly: BoolString;
    objType: string;
    data: SuiObject<T>;
}

export interface SuiObject<T = ContentsDefault> {
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
export const DefaultRpcClient = new SuiRpcClient(rpcUrl);
