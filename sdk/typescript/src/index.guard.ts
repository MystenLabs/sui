/*
 * Generated type guards for "index.ts".
 * WARNING: Do not manually change this file.
 */
import { Ed25519KeypairData, Keypair, PublicKeyInitData, PublicKeyData, SignedTransaction, TransactionResponse, TransactionDigest, GatewayTxSeqNumber, ObjectRef, ObjectExistsInfo, ObjectNotExistsInfo, ObjectStatus, GetObjectInfoResponse, GetOwnedObjectRefsResponse, GetTxnDigestsResponse, TransferTransaction, TxnDataSerializer } from "./index";
import { BN } from "bn.js";

export function isEd25519KeypairData(obj: any, _argumentName?: string): obj is Ed25519KeypairData {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.publicKey instanceof Uint8Array &&
        obj.secretKey instanceof Uint8Array
    )
}

export function isKeypair(obj: any, _argumentName?: string): obj is Keypair {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        typeof obj.getPublicKey === "function" &&
        typeof obj.signData === "function"
    )
}

export function isPublicKeyInitData(obj: any, _argumentName?: string): obj is PublicKeyInitData {
    return (
        (isTransactionResponse(obj) as boolean ||
            isGatewayTxSeqNumber(obj) as boolean ||
            obj instanceof Buffer ||
            obj instanceof Uint8Array ||
            Array.isArray(obj) &&
            obj.every((e: any) =>
                isGatewayTxSeqNumber(e) as boolean
            ) ||
            isPublicKeyData(obj) as boolean)
    )
}

export function isPublicKeyData(obj: any, _argumentName?: string): obj is PublicKeyData {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj._bn instanceof BN
    )
}

export function isSignedTransaction(obj: any, _argumentName?: string): obj is SignedTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.txBytes) as boolean &&
        isTransactionResponse(obj.signature) as boolean &&
        isTransactionResponse(obj.pubKey) as boolean
    )
}

export function isTransactionResponse(obj: any, _argumentName?: string): obj is TransactionResponse {
    return (
        typeof obj === "string"
    )
}

export function isTransactionDigest(obj: any, _argumentName?: string): obj is TransactionDigest {
    return (
        typeof obj === "string"
    )
}

export function isGatewayTxSeqNumber(obj: any, _argumentName?: string): obj is GatewayTxSeqNumber {
    return (
        typeof obj === "number"
    )
}

export function isObjectRef(obj: any, _argumentName?: string): obj is ObjectRef {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.digest) as boolean &&
        isTransactionResponse(obj.objectId) as boolean &&
        isGatewayTxSeqNumber(obj.version) as boolean
    )
}

export function isObjectExistsInfo(obj: any, _argumentName?: string): obj is ObjectExistsInfo {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectRef(obj.objectRef) as boolean
    )
}

export function isObjectNotExistsInfo(obj: any, _argumentName?: string): obj is ObjectNotExistsInfo {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function")
    )
}

export function isObjectStatus(obj: any, _argumentName?: string): obj is ObjectStatus {
    return (
        (obj === "Exists" ||
            obj === "NotExists" ||
            obj === "Deleted")
    )
}

export function isGetObjectInfoResponse(obj: any, _argumentName?: string): obj is GetObjectInfoResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectStatus(obj.status) as boolean &&
        (isObjectRef(obj.details) as boolean ||
            isObjectExistsInfo(obj.details) as boolean ||
            isObjectNotExistsInfo(obj.details) as boolean)
    )
}

export function isGetOwnedObjectRefsResponse(obj: any, _argumentName?: string): obj is GetOwnedObjectRefsResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.objects) &&
        obj.objects.every((e: any) =>
            isObjectRef(e) as boolean
        )
    )
}

export function isGetTxnDigestsResponse(obj: any, _argumentName?: string): obj is GetTxnDigestsResponse {
    return (
        Array.isArray(obj) &&
        isGatewayTxSeqNumber(obj[0]) as boolean &&
        isTransactionResponse(obj[1]) as boolean
    )
}

export function isTransferTransaction(obj: any, _argumentName?: string): obj is TransferTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.fromAddress) as boolean &&
        isTransactionResponse(obj.objectId) as boolean &&
        isTransactionResponse(obj.toAddress) as boolean &&
        isTransactionResponse(obj.gasObjectId) as boolean &&
        isGatewayTxSeqNumber(obj.gas_budget) as boolean
    )
}

export function isTxnDataSerializer(obj: any, _argumentName?: string): obj is TxnDataSerializer {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        typeof obj.new_transfer === "function"
    )
}
