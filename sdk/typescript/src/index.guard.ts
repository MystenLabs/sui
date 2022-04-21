// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * Generated type guards for "index.ts".
 * WARNING: Do not manually change this file.
 */
import { Ed25519KeypairData, Keypair, PublicKeyInitData, PublicKeyData, SignedTransaction, TransactionResponse, TransferTransaction, TxnDataSerializer, GetOwnedObjectRefsResponse, ObjectDigest, ObjectId, SequenceNumber, RawObjectRef, Transfer, RawAuthoritySignInfo, SingleTransactionKind, TransactionKind, TransactionData, Transaction, CertifiedTransaction, MoveModulePublish, MoveTypeTag, MoveCall, EmptySignInfo, AuthorityName, AuthoritySignature } from "./index";
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
            isSequenceNumber(obj) as boolean ||
            obj instanceof Buffer ||
            obj instanceof Uint8Array ||
            Array.isArray(obj) &&
            obj.every((e: any) =>
                isSequenceNumber(e) as boolean
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

export function isTransferTransaction(obj: any, _argumentName?: string): obj is TransferTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.fromAddress) as boolean &&
        isTransactionResponse(obj.objectId) as boolean &&
        isTransactionResponse(obj.toAddress) as boolean &&
        isTransactionResponse(obj.gasObjectId) as boolean &&
        isSequenceNumber(obj.gas_budget) as boolean
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

export function isGetOwnedObjectRefsResponse(obj: any, _argumentName?: string): obj is GetOwnedObjectRefsResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.objects) &&
        obj.objects.every((e: any) =>
            (e !== null &&
                typeof e === "object" ||
                typeof e === "function") &&
            isTransactionResponse(e.digest) as boolean &&
            isTransactionResponse(e.objectId) as boolean &&
            isSequenceNumber(e.version) as boolean
        )
    )
}

export function isObjectDigest(obj: any, _argumentName?: string): obj is ObjectDigest {
    return (
        typeof obj === "string"
    )
}

export function isObjectId(obj: any, _argumentName?: string): obj is ObjectId {
    return (
        typeof obj === "string"
    )
}

export function isSequenceNumber(obj: any, _argumentName?: string): obj is SequenceNumber {
    return (
        typeof obj === "number"
    )
}

export function isRawObjectRef(obj: any, _argumentName?: string): obj is RawObjectRef {
    return (
        Array.isArray(obj) &&
        isTransactionResponse(obj[0]) as boolean &&
        isSequenceNumber(obj[1]) as boolean &&
        isTransactionResponse(obj[2]) as boolean
    )
}

export function isTransfer(obj: any, _argumentName?: string): obj is Transfer {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.recipient) as boolean &&
        isRawObjectRef(obj.object_ref) as boolean
    )
}

export function isRawAuthoritySignInfo(obj: any, _argumentName?: string): obj is RawAuthoritySignInfo {
    return (
        Array.isArray(obj) &&
        isTransactionResponse(obj[0]) as boolean &&
        isTransactionResponse(obj[1]) as boolean
    )
}

export function isSingleTransactionKind(obj: any, _argumentName?: string): obj is SingleTransactionKind {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransfer(obj.Transfer) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isMoveModulePublish(obj.Publish) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isMoveCall(obj.Call) as boolean)
    )
}

export function isTransactionKind(obj: any, _argumentName?: string): obj is TransactionKind {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isSingleTransactionKind(obj.Single) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.Batch) &&
            obj.Batch.every((e: any) =>
                isSingleTransactionKind(e) as boolean
            ))
    )
}

export function isTransactionData(obj: any, _argumentName?: string): obj is TransactionData {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionKind(obj.kind) as boolean &&
        isTransactionResponse(obj.sender) as boolean &&
        isRawObjectRef(obj.gas_payment) as boolean &&
        isSequenceNumber(obj.gas_budget) as boolean
    )
}

export function isTransaction(obj: any, _argumentName?: string): obj is Transaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionData(obj.data) as boolean &&
        isTransactionResponse(obj.tx_signature) as boolean &&
        isTransactionResponse(obj.auth_signature) as boolean
    )
}

export function isCertifiedTransaction(obj: any, _argumentName?: string): obj is CertifiedTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransaction(obj.transaction) as boolean &&
        Array.isArray(obj.signatures) &&
        obj.signatures.every((e: any) =>
            isRawAuthoritySignInfo(e) as boolean
        )
    )
}

export function isMoveModulePublish(obj: any, _argumentName?: string): obj is MoveModulePublish {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function")
    )
}

export function isMoveTypeTag(obj: any, _argumentName?: string): obj is MoveTypeTag {
    return (
        (obj === "bool" ||
            obj === "u8" ||
            obj === "u64" ||
            obj === "u128" ||
            obj === "address" ||
            obj === "signer" ||
            obj === "vector" ||
            obj === "struct")
    )
}

export function isMoveCall(obj: any, _argumentName?: string): obj is MoveCall {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isRawObjectRef(obj.packages) as boolean &&
        isTransactionResponse(obj.module) as boolean &&
        isTransactionResponse(obj.function) as boolean &&
        Array.isArray(obj.type_arguments) &&
        obj.type_arguments.every((e: any) =>
            isMoveTypeTag(e) as boolean
        ) &&
        Array.isArray(obj.object_arguments) &&
        obj.object_arguments.every((e: any) =>
            isRawObjectRef(e) as boolean
        ) &&
        Array.isArray(obj.shared_object_arguments) &&
        obj.shared_object_arguments.every((e: any) =>
            isTransactionResponse(e) as boolean
        ) &&
        Array.isArray(obj.pure_arguments)
    )
}

export function isEmptySignInfo(obj: any, _argumentName?: string): obj is EmptySignInfo {
    return (
        typeof obj === "object"
    )
}

export function isAuthorityName(obj: any, _argumentName?: string): obj is AuthorityName {
    return (
        typeof obj === "string"
    )
}

export function isAuthoritySignature(obj: any, _argumentName?: string): obj is AuthoritySignature {
    return (
        typeof obj === "string"
    )
}
