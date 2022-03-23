import { Console } from "console";

export class SuiRpcClient {
    public readonly host: string;

    readonly moveCallUrl: string;
    readonly addressesUrl: string;

    // TODO - url type for host
    public constructor(host: string) {
        this.host = host;
        this.moveCallUrl = `${host}/wallet/call`;
        this.addressesUrl = `${this.host}/addresses`;
    }

    public getAddresses = async (): Promise<Addresses> =>
        this.fetchJson(this.addressesUrl)

    public getAddressObjects = async (address: SuiAddressHexStr) => {
        const url = `${this.host}/objects?address=${address}`;
        return this.fetchJson(url);
    }

    public async getObjectInfo (id: string): Promise<ObjectInfoResponse<object>> {
        const url = `${this.host}/object_info?objectId=${id}`;
        return this.fetchJson(url);
    }

    public async getObjectInfoT<T extends object> (id: string)
        : Promise<ObjectInfoResponse<T>>
    {
        return await this.getObjectInfo(id) as ObjectInfoResponse<T>;
    }

    // TODO - more detailed type for input
    public async moveCall<TIn extends object>(input: TIn): Promise<MoveCallResponse> {
        return this.postJson(this.moveCallUrl, input);
    }

    async fetchJson(url: string): Promise<any> {
        let response = await fetch(url, { mode: 'cors' });
        switch (response.status) {
            case 200:
                return response.json();
            case 424:
                throw new Error('424 response status - likely requesting missing data!');
            default:
                throw new Error(`unhandled HTTP response code: ${response.status}`);
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
                throw new Error(`non-200 response to POST ${this.moveCallUrl}: ${response.status}`);
        }
    }

    public static modifyForDemo <T extends object, U>(obj: T): T {
        for (var prop in obj) {
            //console.log('obj prop', prop);
            let property = obj[prop];
            //console.log('property', property);

            if (typeof(property) == 'object') {
                if ('bytes' in property) {
                    const pb = property as unknown as JsonHexBytes;
                    if(isValidSuiIdBytes(pb))
                        console.log("valid sui id bytes", pb.bytes);
                }

                this.modifyForDemo(property as unknown as object);
            }
        }

        return obj;
    }
}


export const hexToAscii = function(hex: string) {
    var str = "";
    var i = 0, l = hex.length;
    if (hex.substring(0, 2) === '0x') {
        i = 2;
    }
    for (; i < l; i+=2) {
        var code = parseInt(hex.substr(i, 2), 16);
        str += String.fromCharCode(code);
    }

    return str;
}

const SUI_ADDRESS_LEN = 20;
export type SuiAddressBytes = number[];
export type Signature = number[];

type SuiAddressHexStr = string;

const TX_DIGEST_LEN = 32;
type SuiTxDigest = number[];   // 32 bytes

const hexStringPattern = /$0x[0-9a-fA-F]*^/;
const suiAddressHexPattern = /$0x[0-9a-fA-F]{20}^/;
const isBytesHexStr = (str: string) => hexStringPattern.test(str);
const isSuiAddressHexStr = (str: string) => suiAddressHexPattern.test(str);

const isValidSuiIdBytes = (obj: { bytes: string | number[] }) => {
    const bytesFieldType = typeof obj.bytes;

    if (bytesFieldType === 'object') {
        if (Array.isArray(obj.bytes)) {
            const objBytesAsArray = obj.bytes as number[];
            if(objBytesAsArray.length != SUI_ADDRESS_LEN)
                return false;

            for (let i = 0; i < objBytesAsArray.length; i++) {
                if(objBytesAsArray[i] > 255)
                    return false;
            }
            return true;
        }
        else return false
    }
    else if (bytesFieldType === 'string') {
        return isSuiAddressHexStr(obj.bytes as string);
    }

    return false;
}

export type AddressOwner = { AddressOwner: SuiAddressBytes }
type ObjectOwner = { ObjectOwner: SuiAddressBytes }
export type AnyVec = { vec: any[] }
type BoolString = "true" | "false";
const parseBoolString = (bs: BoolString) => bs === "true" ? true : false;

export type JsonBytes = { bytes: number[] }
export type JsonHexBytes = { bytes: string | number[] }

export type SuiRefHexBytes = { bytes: string }      // TODO - better types for hex strings

export interface SuiParentChildRef {
    child_id: SuiRefHexBytes,
    parent_id: SuiRefHexBytes
}

export type MoveVec = { vec: any[] }
export type TMoveVec<T extends object> = { vec: T[] }

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
    owner: ObjectOwner;
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
    sender: SuiAddressBytes;
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
    addresses: string[]
}

export interface AddressObjectsResponse {
    id?: string,                            // TODO - can we remove this ?
    objects: AddressObjectSummary[]
}

// TODO - this format is inconsistent with other object summaries (camelCase vs snake_case, lack of type field)
// also needs stronger types for fields
export interface AddressObjectSummary {
    objectId: string,
    version: string,
    objectDigest: string
}

export const tryGetRpcParam = (): string | null => {
    const params = new URLSearchParams(window.location.search);

    let rpcParam = null;
    params.forEach((value, key) => {
        if(key === 'rpc') {
            const decoded = decodeURIComponent(value);
            if (isValidHttpUrl(decoded)) {
                rpcParam = decoded;
                window.localStorage.setItem(LOCALSTORE_RPC_KEY, decoded);
                window.localStorage.setItem(LOCALSTORE_RPC_TIME_KEY, Date.now().toString());
            }
        }
    });

    return rpcParam;
}

const LOCALSTORE_RPC_KEY = 'sui-explorer-rpc'
const LOCALSTORE_RPC_TIME_KEY = 'sui-explorer-rpc-lastset'

const LOCALSTORE_RPC_VALID_MS = 60000 * 60 * 3;

// persisting this preference ad-hoc in local storage is to support localhost rpc
const tryGetRpcLocalStorage = (): string | null => {
    let value = window.localStorage.getItem(LOCALSTORE_RPC_KEY);
    const lastUpdated = window.localStorage.getItem(LOCALSTORE_RPC_TIME_KEY);

    if(lastUpdated) {
        console.log(lastUpdated);
        const last = Number.parseInt(lastUpdated);
        const now = Date.now().valueOf();
        console.log(last, now);
        if(now === last)
            return value;

        const elapsed = now.valueOf() - last.valueOf();
        if (elapsed >= LOCALSTORE_RPC_VALID_MS) {
            console.log(`removing stale rpc url preference`);
            window.localStorage.removeItem(LOCALSTORE_RPC_KEY);
            window.localStorage.removeItem(LOCALSTORE_RPC_TIME_KEY);
            value = null;
        }
    }

    return value;
}

export const tryGetRpcSetting = (): string | null => {
    const queryParam = tryGetRpcParam();
    const localStore = tryGetRpcLocalStorage();
    // query param takes precedence over local store
    return queryParam ? queryParam : localStore;
}


const isValidHttpUrl = (url: string) => {
    try { new URL(url); }
    catch (e) { return false; }
    return url.startsWith('http') || url.startsWith('https');
  };


// allow switching the default url with another RPC url (for local testing)
const rpcParam = tryGetRpcSetting();
const rpcUrl = rpcParam ? rpcParam : 'https://demo-rpc.sui.io';

export const DefaultRpcClient = new SuiRpcClient(rpcUrl);
