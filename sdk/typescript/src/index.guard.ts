// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * Generated type guards for "index.ts".
 * WARNING: Do not manually change this file.
 */
import { Ed25519KeypairData, Keypair, PublicKeyInitData, PublicKeyData, TransferObjectTransaction, TransferSuiTransaction, MergeCoinTransaction, SplitCoinTransaction, MoveCallTransaction, PublishTransaction, TxnDataSerializer, SignaturePubkeyPair, Signer, TransactionDigest, SuiAddress, ObjectOwner, SuiObjectRef, SuiObjectInfo, ObjectContentFields, MovePackageContent, SuiData, SuiMoveObject, SuiMovePackage, SuiObject, ObjectStatus, ObjectType, GetOwnedObjectsResponse, GetObjectDataResponse, ObjectDigest, ObjectId, SequenceNumber, TransferObject, SuiTransferSui, SuiChangeEpoch, TransactionKindName, SuiTransactionKind, TransactionData, EpochId, AuthorityQuorumSignInfo, CertifiedTransaction, GasCostSummary, ExecutionStatusType, ExecutionStatus, OwnedObjectRef, TransactionEffects, TransactionEffectsResponse, GatewayTxSeqNumber, GetTxnDigestsResponse, MoveCall, SuiJsonValue, EmptySignInfo, AuthorityName, AuthoritySignature, TransactionBytes, MergeCoinResponse, SplitCoinResponse, PublishResponse, SuiPackage, TransactionResponse } from "./index";
import { BN } from "bn.js";
import { Base64DataBuffer } from "./serialization/base64";
import { PublicKey } from "./cryptography/publickey";

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
        (isTransactionDigest(obj) as boolean ||
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

export function isTransferObjectTransaction(obj: any, _argumentName?: string): obj is TransferObjectTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.objectId) as boolean &&
        (typeof obj.gasPayment === "undefined" ||
            isTransactionDigest(obj.gasPayment) as boolean) &&
        isSequenceNumber(obj.gasBudget) as boolean &&
        isTransactionDigest(obj.recipient) as boolean
    )
}

export function isTransferSuiTransaction(obj: any, _argumentName?: string): obj is TransferSuiTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.suiObjectId) as boolean &&
        isSequenceNumber(obj.gasBudget) as boolean &&
        isTransactionDigest(obj.recipient) as boolean &&
        (typeof obj.amount === "undefined" ||
            isSequenceNumber(obj.amount) as boolean)
    )
}

export function isMergeCoinTransaction(obj: any, _argumentName?: string): obj is MergeCoinTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.primaryCoin) as boolean &&
        isTransactionDigest(obj.coinToMerge) as boolean &&
        (typeof obj.gasPayment === "undefined" ||
            isTransactionDigest(obj.gasPayment) as boolean) &&
        isSequenceNumber(obj.gasBudget) as boolean
    )
}

export function isSplitCoinTransaction(obj: any, _argumentName?: string): obj is SplitCoinTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.coinObjectId) as boolean &&
        Array.isArray(obj.splitAmounts) &&
        obj.splitAmounts.every((e: any) =>
            isSequenceNumber(e) as boolean
        ) &&
        (typeof obj.gasPayment === "undefined" ||
            isTransactionDigest(obj.gasPayment) as boolean) &&
        isSequenceNumber(obj.gasBudget) as boolean
    )
}

export function isMoveCallTransaction(obj: any, _argumentName?: string): obj is MoveCallTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.packageObjectId) as boolean &&
        isTransactionDigest(obj.module) as boolean &&
        isTransactionDigest(obj.function) as boolean &&
        Array.isArray(obj.typeArguments) &&
        obj.typeArguments.every((e: any) =>
            isTransactionDigest(e) as boolean
        ) &&
        Array.isArray(obj.arguments) &&
        obj.arguments.every((e: any) =>
            isSuiJsonValue(e) as boolean
        ) &&
        (typeof obj.gasPayment === "undefined" ||
            isTransactionDigest(obj.gasPayment) as boolean) &&
        isSequenceNumber(obj.gasBudget) as boolean
    )
}

export function isPublishTransaction(obj: any, _argumentName?: string): obj is PublishTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.compiledModules) &&
        obj.compiledModules.every((e: any) =>
            isTransactionDigest(e) as boolean
        ) &&
        (typeof obj.gasPayment === "undefined" ||
            isTransactionDigest(obj.gasPayment) as boolean) &&
        isSequenceNumber(obj.gasBudget) as boolean
    )
}

export function isTxnDataSerializer(obj: any, _argumentName?: string): obj is TxnDataSerializer {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        typeof obj.newTransferObject === "function" &&
        typeof obj.newTransferSui === "function" &&
        typeof obj.newMoveCall === "function" &&
        typeof obj.newMergeCoin === "function" &&
        typeof obj.newSplitCoin === "function" &&
        typeof obj.newPublish === "function"
    )
}

export function isSignaturePubkeyPair(obj: any, _argumentName?: string): obj is SignaturePubkeyPair {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.flag instanceof Base64DataBuffer &&
        obj.signature instanceof Base64DataBuffer &&
        obj.pubKey instanceof PublicKey
    )
}

export function isSigner(obj: any, _argumentName?: string): obj is Signer {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        typeof obj.getAddress === "function" &&
        typeof obj.signData === "function"
    )
}

export function isTransactionDigest(obj: any, _argumentName?: string): obj is TransactionDigest {
    return (
        typeof obj === "string"
    )
}

export function isSuiAddress(obj: any, _argumentName?: string): obj is SuiAddress {
    return (
        typeof obj === "string"
    )
}

export function isObjectOwner(obj: any, _argumentName?: string): obj is ObjectOwner {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransactionDigest(obj.AddressOwner) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.ObjectOwner) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.SingleOwner) as boolean ||
            obj === "Shared" ||
            obj === "Immutable")
    )
}

export function isSuiObjectRef(obj: any, _argumentName?: string): obj is SuiObjectRef {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.digest) as boolean &&
        isTransactionDigest(obj.objectId) as boolean &&
        isSequenceNumber(obj.version) as boolean
    )
}

export function isSuiObjectInfo(obj: any, _argumentName?: string): obj is SuiObjectInfo {
    return (
        isSuiObjectRef(obj) as boolean &&
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.type) as boolean &&
        isObjectOwner(obj.owner) as boolean &&
        isTransactionDigest(obj.previousTransaction) as boolean
    )
}

export function isObjectContentFields(obj: any, _argumentName?: string): obj is ObjectContentFields {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Object.entries<any>(obj)
            .every(([key, _value]) => (isTransactionDigest(key) as boolean))
    )
}

export function isMovePackageContent(obj: any, _argumentName?: string): obj is MovePackageContent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Object.entries<any>(obj)
            .every(([key, value]) => (isTransactionDigest(value) as boolean &&
                isTransactionDigest(key) as boolean))
    )
}

export function isSuiData(obj: any, _argumentName?: string): obj is SuiData {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isObjectType(obj.dataType) as boolean &&
            isSuiMoveObject(obj) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isObjectType(obj.dataType) as boolean &&
            isSuiMovePackage(obj) as boolean)
    )
}

export function isSuiMoveObject(obj: any, _argumentName?: string): obj is SuiMoveObject {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.type) as boolean &&
        isObjectContentFields(obj.fields) as boolean &&
        (typeof obj.has_public_transfer === "undefined" ||
            obj.has_public_transfer === false ||
            obj.has_public_transfer === true)
    )
}

export function isSuiMovePackage(obj: any, _argumentName?: string): obj is SuiMovePackage {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isMovePackageContent(obj.disassembled) as boolean
    )
}

export function isSuiObject(obj: any, _argumentName?: string): obj is SuiObject {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiData(obj.data) as boolean &&
        isObjectOwner(obj.owner) as boolean &&
        isTransactionDigest(obj.previousTransaction) as boolean &&
        isSequenceNumber(obj.storageRebate) as boolean &&
        isSuiObjectRef(obj.reference) as boolean
    )
}

export function isObjectStatus(obj: any, _argumentName?: string): obj is ObjectStatus {
    return (
        (obj === "Exists" ||
            obj === "NotExists" ||
            obj === "Deleted")
    )
}

export function isObjectType(obj: any, _argumentName?: string): obj is ObjectType {
    return (
        (obj === "moveObject" ||
            obj === "package")
    )
}

export function isGetOwnedObjectsResponse(obj: any, _argumentName?: string): obj is GetOwnedObjectsResponse {
    return (
        Array.isArray(obj) &&
        obj.every((e: any) =>
            isSuiObjectInfo(e) as boolean
        )
    )
}

export function isGetObjectDataResponse(obj: any, _argumentName?: string): obj is GetObjectDataResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectStatus(obj.status) as boolean &&
        (isTransactionDigest(obj.details) as boolean ||
            isSuiObjectRef(obj.details) as boolean ||
            isSuiObject(obj.details) as boolean)
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

export function isTransferObject(obj: any, _argumentName?: string): obj is TransferObject {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.recipient) as boolean &&
        isSuiObjectRef(obj.objectRef) as boolean
    )
}

export function isSuiTransferSui(obj: any, _argumentName?: string): obj is SuiTransferSui {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.recipient) as boolean &&
        (obj.amount === null ||
            isSequenceNumber(obj.amount) as boolean)
    )
}

export function isSuiChangeEpoch(obj: any, _argumentName?: string): obj is SuiChangeEpoch {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSequenceNumber(obj.epoch) as boolean &&
        isSequenceNumber(obj.storage_charge) as boolean &&
        isSequenceNumber(obj.computation_charge) as boolean
    )
}

export function isTransactionKindName(obj: any, _argumentName?: string): obj is TransactionKindName {
    return (
        (obj === "TransferObject" ||
            obj === "Publish" ||
            obj === "Call" ||
            obj === "TransferSui" ||
            obj === "ChangeEpoch")
    )
}

export function isSuiTransactionKind(obj: any, _argumentName?: string): obj is SuiTransactionKind {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransferObject(obj.TransferObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiMovePackage(obj.Publish) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isMoveCall(obj.Call) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiTransferSui(obj.TransferSui) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiChangeEpoch(obj.ChangeEpoch) as boolean)
    )
}

export function isTransactionData(obj: any, _argumentName?: string): obj is TransactionData {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.transactions) &&
        obj.transactions.every((e: any) =>
            isSuiTransactionKind(e) as boolean
        ) &&
        isTransactionDigest(obj.sender) as boolean &&
        isSuiObjectRef(obj.gasPayment) as boolean &&
        isSequenceNumber(obj.gasBudget) as boolean
    )
}

export function isEpochId(obj: any, _argumentName?: string): obj is EpochId {
    return (
        typeof obj === "number"
    )
}

export function isAuthorityQuorumSignInfo(obj: any, _argumentName?: string): obj is AuthorityQuorumSignInfo {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSequenceNumber(obj.epoch) as boolean &&
        Array.isArray(obj.signature) &&
        obj.signature.every((e: any) =>
            isTransactionDigest(e) as boolean
        )
    )
}

export function isCertifiedTransaction(obj: any, _argumentName?: string): obj is CertifiedTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.transactionDigest) as boolean &&
        isTransactionData(obj.data) as boolean &&
        isTransactionDigest(obj.txSignature) as boolean &&
        isAuthorityQuorumSignInfo(obj.authSignInfo) as boolean
    )
}

export function isGasCostSummary(obj: any, _argumentName?: string): obj is GasCostSummary {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSequenceNumber(obj.computationCost) as boolean &&
        isSequenceNumber(obj.storageCost) as boolean &&
        isSequenceNumber(obj.storageRebate) as boolean
    )
}

export function isExecutionStatusType(obj: any, _argumentName?: string): obj is ExecutionStatusType {
    return (
        (obj === "success" ||
            obj === "failure")
    )
}

export function isExecutionStatus(obj: any, _argumentName?: string): obj is ExecutionStatus {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isExecutionStatusType(obj.status) as boolean &&
        (typeof obj.error === "undefined" ||
            isTransactionDigest(obj.error) as boolean)
    )
}

export function isOwnedObjectRef(obj: any, _argumentName?: string): obj is OwnedObjectRef {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectOwner(obj.owner) as boolean &&
        isSuiObjectRef(obj.reference) as boolean
    )
}

export function isTransactionEffects(obj: any, _argumentName?: string): obj is TransactionEffects {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isExecutionStatus(obj.status) as boolean &&
        isGasCostSummary(obj.gasUsed) as boolean &&
        (typeof obj.sharedObjects === "undefined" ||
            Array.isArray(obj.sharedObjects) &&
            obj.sharedObjects.every((e: any) =>
                isSuiObjectRef(e) as boolean
            )) &&
        isTransactionDigest(obj.transactionDigest) as boolean &&
        (typeof obj.created === "undefined" ||
            Array.isArray(obj.created) &&
            obj.created.every((e: any) =>
                isOwnedObjectRef(e) as boolean
            )) &&
        (typeof obj.mutated === "undefined" ||
            Array.isArray(obj.mutated) &&
            obj.mutated.every((e: any) =>
                isOwnedObjectRef(e) as boolean
            )) &&
        (typeof obj.unwrapped === "undefined" ||
            Array.isArray(obj.unwrapped) &&
            obj.unwrapped.every((e: any) =>
                isOwnedObjectRef(e) as boolean
            )) &&
        (typeof obj.deleted === "undefined" ||
            Array.isArray(obj.deleted) &&
            obj.deleted.every((e: any) =>
                isSuiObjectRef(e) as boolean
            )) &&
        (typeof obj.wrapped === "undefined" ||
            Array.isArray(obj.wrapped) &&
            obj.wrapped.every((e: any) =>
                isSuiObjectRef(e) as boolean
            )) &&
        isOwnedObjectRef(obj.gasObject) as boolean &&
        (typeof obj.events === "undefined" ||
            Array.isArray(obj.events)) &&
        (typeof obj.dependencies === "undefined" ||
            Array.isArray(obj.dependencies) &&
            obj.dependencies.every((e: any) =>
                isTransactionDigest(e) as boolean
            ))
    )
}

export function isTransactionEffectsResponse(obj: any, _argumentName?: string): obj is TransactionEffectsResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isCertifiedTransaction(obj.certificate) as boolean &&
        isTransactionEffects(obj.effects) as boolean &&
        (obj.timestamp_ms === null ||
            isSequenceNumber(obj.timestamp_ms) as boolean)
    )
}

export function isGatewayTxSeqNumber(obj: any, _argumentName?: string): obj is GatewayTxSeqNumber {
    return (
        typeof obj === "number"
    )
}

export function isGetTxnDigestsResponse(obj: any, _argumentName?: string): obj is GetTxnDigestsResponse {
    return (
        Array.isArray(obj) &&
        obj.every((e: any) =>
            Array.isArray(e) &&
            isSequenceNumber(e[0]) as boolean &&
            isTransactionDigest(e[1]) as boolean
        )
    )
}

export function isMoveCall(obj: any, _argumentName?: string): obj is MoveCall {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiObjectRef(obj.package) as boolean &&
        isTransactionDigest(obj.module) as boolean &&
        isTransactionDigest(obj.function) as boolean &&
        (typeof obj.typeArguments === "undefined" ||
            Array.isArray(obj.typeArguments) &&
            obj.typeArguments.every((e: any) =>
                isTransactionDigest(e) as boolean
            )) &&
        (typeof obj.arguments === "undefined" ||
            Array.isArray(obj.arguments) &&
            obj.arguments.every((e: any) =>
                isSuiJsonValue(e) as boolean
            ))
    )
}

export function isSuiJsonValue(obj: any, _argumentName?: string): obj is SuiJsonValue {
    return (
        (isTransactionDigest(obj) as boolean ||
            isSequenceNumber(obj) as boolean ||
            obj === false ||
            obj === true ||
            Array.isArray(obj) &&
            obj.every((e: any) =>
            (isTransactionDigest(e) as boolean ||
                isSequenceNumber(e) as boolean ||
                e === false ||
                e === true)
            ))
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

export function isTransactionBytes(obj: any, _argumentName?: string): obj is TransactionBytes {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.txBytes) as boolean &&
        isSuiObjectRef(obj.gas) as boolean
    )
}

export function isMergeCoinResponse(obj: any, _argumentName?: string): obj is MergeCoinResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isCertifiedTransaction(obj.certificate) as boolean &&
        isSuiObject(obj.updatedCoin) as boolean &&
        isSuiObject(obj.updatedGas) as boolean
    )
}

export function isSplitCoinResponse(obj: any, _argumentName?: string): obj is SplitCoinResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isCertifiedTransaction(obj.certificate) as boolean &&
        isSuiObject(obj.updatedCoin) as boolean &&
        Array.isArray(obj.newCoins) &&
        obj.newCoins.every((e: any) =>
            isSuiObject(e) as boolean
        ) &&
        isSuiObject(obj.updatedGas) as boolean
    )
}

export function isPublishResponse(obj: any, _argumentName?: string): obj is PublishResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isCertifiedTransaction(obj.certificate) as boolean &&
        Array.isArray(obj.createdObjects) &&
        obj.createdObjects.every((e: any) =>
            isSuiObject(e) as boolean
        ) &&
        isSuiPackage(obj.package) as boolean &&
        isSuiObject(obj.updatedGas) as boolean
    )
}

export function isSuiPackage(obj: any, _argumentName?: string): obj is SuiPackage {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.digest) as boolean &&
        isTransactionDigest(obj.objectId) as boolean &&
        isSequenceNumber(obj.version) as boolean
    )
}

export function isTransactionResponse(obj: any, _argumentName?: string): obj is TransactionResponse {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransactionEffectsResponse(obj.EffectResponse) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSplitCoinResponse(obj.SplitCoinResponse) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isMergeCoinResponse(obj.MergeCoinResponse) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isPublishResponse(obj.PublishResponse) as boolean)
    )
}
