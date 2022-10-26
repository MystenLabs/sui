// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/* eslint-disable */

/*
 * Generated type guards for "index.ts".
 * WARNING: Do not manually change this file.
 */
import { TransactionDigest, SuiAddress, ObjectOwner, SuiObjectRef, SuiObjectInfo, ObjectContentFields, MovePackageContent, SuiData, SuiMoveObject, CoinDenominationInfoResponse, SuiMovePackage, SuiMoveFunctionArgTypesResponse, SuiMoveFunctionArgType, SuiMoveFunctionArgTypes, SuiMoveNormalizedModules, SuiMoveNormalizedModule, SuiMoveModuleId, SuiMoveNormalizedStruct, SuiMoveStructTypeParameter, SuiMoveNormalizedField, SuiMoveNormalizedFunction, SuiMoveVisibility, SuiMoveTypeParameterIndex, SuiMoveAbilitySet, SuiMoveNormalizedType, SuiMoveNormalizedTypeParameterType, SuiMoveNormalizedStructType, SuiObject, ObjectStatus, ObjectType, GetOwnedObjectsResponse, GetObjectDataResponse, ObjectDigest, ObjectId, SequenceNumber, MoveEvent, PublishEvent, TransferObjectEvent, DeleteObjectEvent, NewObjectEvent, SuiEvent, MoveEventField, EventType, SuiEventFilter, SuiEventEnvelope, SuiEvents, SubscriptionId, SubscriptionEvent, TransferObject, SuiTransferSui, SuiChangeEpoch, Pay, ExecuteTransactionRequestType, TransactionKindName, SuiTransactionKind, SuiTransactionData, EpochId, GenericAuthoritySignature, AuthorityQuorumSignInfo, CertifiedTransaction, GasCostSummary, ExecutionStatusType, ExecutionStatus, OwnedObjectRef, TransactionEffects, SuiTransactionResponse, SuiCertifiedTransactionEffects, SuiExecuteTransactionResponse, GatewayTxSeqNumber, GetTxnDigestsResponse, PaginatedTransactionDigests, TransactionQuery, Ordering, MoveCall, SuiJsonValue, EmptySignInfo, AuthorityName, AuthoritySignature, TransactionBytes, SuiParsedMergeCoinResponse, SuiParsedSplitCoinResponse, SuiParsedPublishResponse, SuiPackage, SuiParsedTransactionResponse, DelegationData, DelegationSuiObject, TransferObjectTx, TransferSuiTx, PayTx, PublishTx, SharedObjectRef, ObjectArg, CallArg, StructTag, TypeTag, MoveCallTx, Transaction, TransactionKind, TransactionData, RpcApiVersion } from "./index";

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
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            (obj.Shared !== null &&
                typeof obj.Shared === "object" ||
                typeof obj.Shared === "function") &&
            isSuiMoveTypeParameterIndex(obj.Shared.initial_shared_version) as boolean ||
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
        isSuiMoveTypeParameterIndex(obj.version) as boolean
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

export function isCoinDenominationInfoResponse(obj: any, _argumentName?: string): obj is CoinDenominationInfoResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.coinType) as boolean &&
        (typeof obj.basicUnit === "undefined" ||
            isTransactionDigest(obj.basicUnit) as boolean) &&
        isSuiMoveTypeParameterIndex(obj.decimalNumber) as boolean
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

export function isSuiMoveFunctionArgTypesResponse(obj: any, _argumentName?: string): obj is SuiMoveFunctionArgTypesResponse {
    return (
        Array.isArray(obj) &&
        obj.every((e: any) =>
            isSuiMoveFunctionArgType(e) as boolean
        )
    )
}

export function isSuiMoveFunctionArgType(obj: any, _argumentName?: string): obj is SuiMoveFunctionArgType {
    return (
        (isTransactionDigest(obj) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.Object) as boolean)
    )
}

export function isSuiMoveFunctionArgTypes(obj: any, _argumentName?: string): obj is SuiMoveFunctionArgTypes {
    return (
        Array.isArray(obj) &&
        obj.every((e: any) =>
            isSuiMoveFunctionArgType(e) as boolean
        )
    )
}

export function isSuiMoveNormalizedModules(obj: any, _argumentName?: string): obj is SuiMoveNormalizedModules {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Object.entries<any>(obj)
            .every(([key, value]) => (isSuiMoveNormalizedModule(value) as boolean &&
                isTransactionDigest(key) as boolean))
    )
}

export function isSuiMoveNormalizedModule(obj: any, _argumentName?: string): obj is SuiMoveNormalizedModule {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.file_format_version) as boolean &&
        isTransactionDigest(obj.address) as boolean &&
        isTransactionDigest(obj.name) as boolean &&
        Array.isArray(obj.friends) &&
        obj.friends.every((e: any) =>
            isSuiMoveModuleId(e) as boolean
        ) &&
        (obj.structs !== null &&
            typeof obj.structs === "object" ||
            typeof obj.structs === "function") &&
        Object.entries<any>(obj.structs)
            .every(([key, value]) => (isSuiMoveNormalizedStruct(value) as boolean &&
                isTransactionDigest(key) as boolean)) &&
        (obj.exposed_functions !== null &&
            typeof obj.exposed_functions === "object" ||
            typeof obj.exposed_functions === "function") &&
        Object.entries<any>(obj.exposed_functions)
            .every(([key, value]) => (isSuiMoveNormalizedFunction(value) as boolean &&
                isTransactionDigest(key) as boolean))
    )
}

export function isSuiMoveModuleId(obj: any, _argumentName?: string): obj is SuiMoveModuleId {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.address) as boolean &&
        isTransactionDigest(obj.name) as boolean
    )
}

export function isSuiMoveNormalizedStruct(obj: any, _argumentName?: string): obj is SuiMoveNormalizedStruct {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveAbilitySet(obj.abilities) as boolean &&
        Array.isArray(obj.type_parameters) &&
        obj.type_parameters.every((e: any) =>
            isSuiMoveStructTypeParameter(e) as boolean
        ) &&
        Array.isArray(obj.fields) &&
        obj.fields.every((e: any) =>
            isSuiMoveNormalizedField(e) as boolean
        )
    )
}

export function isSuiMoveStructTypeParameter(obj: any, _argumentName?: string): obj is SuiMoveStructTypeParameter {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveAbilitySet(obj.constraints) as boolean &&
        typeof obj.is_phantom === "boolean"
    )
}

export function isSuiMoveNormalizedField(obj: any, _argumentName?: string): obj is SuiMoveNormalizedField {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.name) as boolean &&
        isSuiMoveNormalizedType(obj.type_) as boolean
    )
}

export function isSuiMoveNormalizedFunction(obj: any, _argumentName?: string): obj is SuiMoveNormalizedFunction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveVisibility(obj.visibility) as boolean &&
        typeof obj.is_entry === "boolean" &&
        Array.isArray(obj.type_parameters) &&
        obj.type_parameters.every((e: any) =>
            isSuiMoveAbilitySet(e) as boolean
        ) &&
        Array.isArray(obj.parameters) &&
        obj.parameters.every((e: any) =>
            isSuiMoveNormalizedType(e) as boolean
        ) &&
        Array.isArray(obj.return_) &&
        obj.return_.every((e: any) =>
            isSuiMoveNormalizedType(e) as boolean
        )
    )
}

export function isSuiMoveVisibility(obj: any, _argumentName?: string): obj is SuiMoveVisibility {
    return (
        (obj === "Private" ||
            obj === "Public" ||
            obj === "Friend")
    )
}

export function isSuiMoveTypeParameterIndex(obj: any, _argumentName?: string): obj is SuiMoveTypeParameterIndex {
    return (
        typeof obj === "number"
    )
}

export function isSuiMoveAbilitySet(obj: any, _argumentName?: string): obj is SuiMoveAbilitySet {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.abilities) &&
        obj.abilities.every((e: any) =>
            isTransactionDigest(e) as boolean
        )
    )
}

export function isSuiMoveNormalizedType(obj: any, _argumentName?: string): obj is SuiMoveNormalizedType {
    return (
        (isTransactionDigest(obj) as boolean ||
            isSuiMoveNormalizedTypeParameterType(obj) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiMoveNormalizedType(obj.Reference) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiMoveNormalizedType(obj.MutableReference) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiMoveNormalizedType(obj.Vector) as boolean ||
            isSuiMoveNormalizedStructType(obj) as boolean)
    )
}

export function isSuiMoveNormalizedTypeParameterType(obj: any, _argumentName?: string): obj is SuiMoveNormalizedTypeParameterType {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.TypeParameter) as boolean
    )
}

export function isSuiMoveNormalizedStructType(obj: any, _argumentName?: string): obj is SuiMoveNormalizedStructType {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.Struct !== null &&
            typeof obj.Struct === "object" ||
            typeof obj.Struct === "function") &&
        isTransactionDigest(obj.Struct.address) as boolean &&
        isTransactionDigest(obj.Struct.module) as boolean &&
        isTransactionDigest(obj.Struct.name) as boolean &&
        Array.isArray(obj.Struct.type_arguments) &&
        obj.Struct.type_arguments.every((e: any) =>
            isSuiMoveNormalizedType(e) as boolean
        )
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
        isSuiMoveTypeParameterIndex(obj.storageRebate) as boolean &&
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

export function isMoveEvent(obj: any, _argumentName?: string): obj is MoveEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.packageId) as boolean &&
        isTransactionDigest(obj.transactionModule) as boolean &&
        isTransactionDigest(obj.sender) as boolean &&
        isTransactionDigest(obj.type) as boolean &&
        (typeof obj.fields === "undefined" ||
            (obj.fields !== null &&
                typeof obj.fields === "object" ||
                typeof obj.fields === "function")) &&
        isTransactionDigest(obj.bcs) as boolean
    )
}

export function isPublishEvent(obj: any, _argumentName?: string): obj is PublishEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.sender) as boolean &&
        isTransactionDigest(obj.packageId) as boolean
    )
}

export function isTransferObjectEvent(obj: any, _argumentName?: string): obj is TransferObjectEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.packageId) as boolean &&
        isTransactionDigest(obj.transactionModule) as boolean &&
        isTransactionDigest(obj.sender) as boolean &&
        isObjectOwner(obj.recipient) as boolean &&
        isTransactionDigest(obj.objectId) as boolean &&
        isSuiMoveTypeParameterIndex(obj.version) as boolean &&
        isTransactionDigest(obj.type) as boolean &&
        (obj.amount === null ||
            isSuiMoveTypeParameterIndex(obj.amount) as boolean)
    )
}

export function isDeleteObjectEvent(obj: any, _argumentName?: string): obj is DeleteObjectEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.packageId) as boolean &&
        isTransactionDigest(obj.transactionModule) as boolean &&
        isTransactionDigest(obj.sender) as boolean &&
        isTransactionDigest(obj.objectId) as boolean
    )
}

export function isNewObjectEvent(obj: any, _argumentName?: string): obj is NewObjectEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.packageId) as boolean &&
        isTransactionDigest(obj.transactionModule) as boolean &&
        isTransactionDigest(obj.sender) as boolean &&
        isObjectOwner(obj.recipient) as boolean &&
        isTransactionDigest(obj.objectId) as boolean
    )
}

export function isSuiEvent(obj: any, _argumentName?: string): obj is SuiEvent {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isMoveEvent(obj.moveEvent) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isPublishEvent(obj.publish) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransferObjectEvent(obj.transferObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isDeleteObjectEvent(obj.deleteObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isNewObjectEvent(obj.newObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            typeof obj.epochChange === "bigint" ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            typeof obj.checkpoint === "bigint")
    )
}

export function isMoveEventField(obj: any, _argumentName?: string): obj is MoveEventField {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.path) as boolean &&
        isSuiJsonValue(obj.value) as boolean
    )
}

export function isEventType(obj: any, _argumentName?: string): obj is EventType {
    return (
        (obj === "MoveEvent" ||
            obj === "Publish" ||
            obj === "TransferObject" ||
            obj === "DeleteObject" ||
            obj === "NewObject" ||
            obj === "EpochChange" ||
            obj === "Checkpoint")
    )
}

export function isSuiEventFilter(obj: any, _argumentName?: string): obj is SuiEventFilter {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransactionDigest(obj.Package) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.Module) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.MoveEventType) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isMoveEventField(obj.MoveEventField) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.SenderAddress) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isEventType(obj.EventType) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.All) &&
            obj.All.every((e: any) =>
                isSuiEventFilter(e) as boolean
            ) ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.Any) &&
            obj.Any.every((e: any) =>
                isSuiEventFilter(e) as boolean
            ) ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.And) &&
            isSuiEventFilter(obj.And[0]) as boolean &&
            isSuiEventFilter(obj.And[1]) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.Or) &&
            isSuiEventFilter(obj.Or[0]) as boolean &&
            isSuiEventFilter(obj.Or[1]) as boolean)
    )
}

export function isSuiEventEnvelope(obj: any, _argumentName?: string): obj is SuiEventEnvelope {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.timestamp) as boolean &&
        isTransactionDigest(obj.txDigest) as boolean &&
        isSuiEvent(obj.event) as boolean
    )
}

export function isSuiEvents(obj: any, _argumentName?: string): obj is SuiEvents {
    return (
        Array.isArray(obj) &&
        obj.every((e: any) =>
            isSuiEventEnvelope(e) as boolean
        )
    )
}

export function isSubscriptionId(obj: any, _argumentName?: string): obj is SubscriptionId {
    return (
        typeof obj === "number"
    )
}

export function isSubscriptionEvent(obj: any, _argumentName?: string): obj is SubscriptionEvent {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.subscription) as boolean &&
        isSuiEventEnvelope(obj.result) as boolean
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
            isSuiMoveTypeParameterIndex(obj.amount) as boolean)
    )
}

export function isSuiChangeEpoch(obj: any, _argumentName?: string): obj is SuiChangeEpoch {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.epoch) as boolean &&
        isSuiMoveTypeParameterIndex(obj.storage_charge) as boolean &&
        isSuiMoveTypeParameterIndex(obj.computation_charge) as boolean
    )
}

export function isPay(obj: any, _argumentName?: string): obj is Pay {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.coins) &&
        obj.coins.every((e: any) =>
            isSuiObjectRef(e) as boolean
        ) &&
        Array.isArray(obj.recipients) &&
        obj.recipients.every((e: any) =>
            isTransactionDigest(e) as boolean
        ) &&
        Array.isArray(obj.amounts) &&
        obj.amounts.every((e: any) =>
            isSuiMoveTypeParameterIndex(e) as boolean
        )
    )
}

export function isExecuteTransactionRequestType(obj: any, _argumentName?: string): obj is ExecuteTransactionRequestType {
    return (
        (obj === "ImmediateReturn" ||
            obj === "WaitForTxCert" ||
            obj === "WaitForEffectsCert" ||
            obj === "WaitForLocalExecution")
    )
}

export function isTransactionKindName(obj: any, _argumentName?: string): obj is TransactionKindName {
    return (
        (obj === "Publish" ||
            obj === "TransferObject" ||
            obj === "Call" ||
            obj === "TransferSui" ||
            obj === "ChangeEpoch" ||
            obj === "Pay")
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
            isSuiChangeEpoch(obj.ChangeEpoch) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isPay(obj.Pay) as boolean)
    )
}

export function isSuiTransactionData(obj: any, _argumentName?: string): obj is SuiTransactionData {
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
        isSuiMoveTypeParameterIndex(obj.gasBudget) as boolean
    )
}

export function isEpochId(obj: any, _argumentName?: string): obj is EpochId {
    return (
        typeof obj === "number"
    )
}

export function isGenericAuthoritySignature(obj: any, _argumentName?: string): obj is GenericAuthoritySignature {
    return (
        (isTransactionDigest(obj) as boolean ||
            Array.isArray(obj) &&
            obj.every((e: any) =>
                isTransactionDigest(e) as boolean
            ))
    )
}

export function isAuthorityQuorumSignInfo(obj: any, _argumentName?: string): obj is AuthorityQuorumSignInfo {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.epoch) as boolean &&
        isGenericAuthoritySignature(obj.signature) as boolean
    )
}

export function isCertifiedTransaction(obj: any, _argumentName?: string): obj is CertifiedTransaction {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.transactionDigest) as boolean &&
        isSuiTransactionData(obj.data) as boolean &&
        isTransactionDigest(obj.txSignature) as boolean &&
        isAuthorityQuorumSignInfo(obj.authSignInfo) as boolean
    )
}

export function isGasCostSummary(obj: any, _argumentName?: string): obj is GasCostSummary {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.computationCost) as boolean &&
        isSuiMoveTypeParameterIndex(obj.storageCost) as boolean &&
        isSuiMoveTypeParameterIndex(obj.storageRebate) as boolean
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

export function isSuiTransactionResponse(obj: any, _argumentName?: string): obj is SuiTransactionResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isCertifiedTransaction(obj.certificate) as boolean &&
        isTransactionEffects(obj.effects) as boolean &&
        (obj.timestamp_ms === null ||
            isSuiMoveTypeParameterIndex(obj.timestamp_ms) as boolean) &&
        (obj.parsed_data === null ||
            (obj.parsed_data !== null &&
                typeof obj.parsed_data === "object" ||
                typeof obj.parsed_data === "function") &&
            isSuiParsedSplitCoinResponse(obj.parsed_data.SplitCoin) as boolean ||
            (obj.parsed_data !== null &&
                typeof obj.parsed_data === "object" ||
                typeof obj.parsed_data === "function") &&
            isSuiParsedMergeCoinResponse(obj.parsed_data.MergeCoin) as boolean ||
            (obj.parsed_data !== null &&
                typeof obj.parsed_data === "object" ||
                typeof obj.parsed_data === "function") &&
            isSuiParsedPublishResponse(obj.parsed_data.Publish) as boolean)
    )
}

export function isSuiCertifiedTransactionEffects(obj: any, _argumentName?: string): obj is SuiCertifiedTransactionEffects {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionEffects(obj.effects) as boolean
    )
}

export function isSuiExecuteTransactionResponse(obj: any, _argumentName?: string): obj is SuiExecuteTransactionResponse {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            (obj.ImmediateReturn !== null &&
                typeof obj.ImmediateReturn === "object" ||
                typeof obj.ImmediateReturn === "function") &&
            isTransactionDigest(obj.ImmediateReturn.tx_digest) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            (obj.TxCert !== null &&
                typeof obj.TxCert === "object" ||
                typeof obj.TxCert === "function") &&
            isCertifiedTransaction(obj.TxCert.certificate) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            (obj.EffectsCert !== null &&
                typeof obj.EffectsCert === "object" ||
                typeof obj.EffectsCert === "function") &&
            isCertifiedTransaction(obj.EffectsCert.certificate) as boolean &&
            isSuiCertifiedTransactionEffects(obj.EffectsCert.effects) as boolean)
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
            isTransactionDigest(e) as boolean
        )
    )
}

export function isPaginatedTransactionDigests(obj: any, _argumentName?: string): obj is PaginatedTransactionDigests {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        Array.isArray(obj.data) &&
        obj.data.every((e: any) =>
            isTransactionDigest(e) as boolean
        ) &&
        (obj.nextCursor === null ||
            isTransactionDigest(obj.nextCursor) as boolean)
    )
}

export function isTransactionQuery(obj: any, _argumentName?: string): obj is TransactionQuery {
    return (
        (obj === "All" ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            (obj.MoveFunction !== null &&
                typeof obj.MoveFunction === "object" ||
                typeof obj.MoveFunction === "function") &&
            isTransactionDigest(obj.MoveFunction.package) as boolean &&
            (obj.MoveFunction.module === null ||
                isTransactionDigest(obj.MoveFunction.module) as boolean) &&
            (obj.MoveFunction.function === null ||
                isTransactionDigest(obj.MoveFunction.function) as boolean) ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.InputObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.MutatedObject) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.FromAddress) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.ToAddress) as boolean)
    )
}

export function isOrdering(obj: any, _argumentName?: string): obj is Ordering {
    return (
        (obj === "Ascending" ||
            obj === "Descending")
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
            isSuiMoveTypeParameterIndex(obj) as boolean ||
            obj === false ||
            obj === true ||
            Array.isArray(obj) &&
            obj.every((e: any) =>
                isSuiJsonValue(e) as boolean
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

export function isSuiParsedMergeCoinResponse(obj: any, _argumentName?: string): obj is SuiParsedMergeCoinResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiObject(obj.updatedCoin) as boolean &&
        isSuiObject(obj.updatedGas) as boolean
    )
}

export function isSuiParsedSplitCoinResponse(obj: any, _argumentName?: string): obj is SuiParsedSplitCoinResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiObject(obj.updatedCoin) as boolean &&
        Array.isArray(obj.newCoins) &&
        obj.newCoins.every((e: any) =>
            isSuiObject(e) as boolean
        ) &&
        isSuiObject(obj.updatedGas) as boolean
    )
}

export function isSuiParsedPublishResponse(obj: any, _argumentName?: string): obj is SuiParsedPublishResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
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
        isSuiMoveTypeParameterIndex(obj.version) as boolean
    )
}

export function isSuiParsedTransactionResponse(obj: any, _argumentName?: string): obj is SuiParsedTransactionResponse {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isSuiParsedSplitCoinResponse(obj.SplitCoin) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiParsedMergeCoinResponse(obj.MergeCoin) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSuiParsedPublishResponse(obj.Publish) as boolean)
    )
}

export function isDelegationData(obj: any, _argumentName?: string): obj is DelegationData {
    return (
        isSuiMoveObject(obj) as boolean &&
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectType(obj.dataType) as boolean &&
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.type === "0x2::delegation::Delegation" &&
        (obj.fields !== null &&
            typeof obj.fields === "object" ||
            typeof obj.fields === "function") &&
        (isSuiMoveTypeParameterIndex(obj.fields.active_delegation) as boolean ||
            (obj.fields.active_delegation !== null &&
                typeof obj.fields.active_delegation === "object" ||
                typeof obj.fields.active_delegation === "function") &&
            (obj.fields.active_delegation.fields !== null &&
                typeof obj.fields.active_delegation.fields === "object" ||
                typeof obj.fields.active_delegation.fields === "function") &&
            obj.fields.active_delegation.fields.vec === "" &&
            isTransactionDigest(obj.fields.active_delegation.type) as boolean) &&
        isSuiMoveTypeParameterIndex(obj.fields.delegate_amount) as boolean &&
        isSuiMoveTypeParameterIndex(obj.fields.next_reward_unclaimed_epoch) as boolean &&
        isTransactionDigest(obj.fields.validator_address) as boolean &&
        (obj.fields.info !== null &&
            typeof obj.fields.info === "object" ||
            typeof obj.fields.info === "function") &&
        isTransactionDigest(obj.fields.info.id) as boolean &&
        isSuiMoveTypeParameterIndex(obj.fields.info.version) as boolean &&
        (isSuiMoveObject(obj.fields.coin_locked_until_epoch) as boolean ||
            (obj.fields.coin_locked_until_epoch !== null &&
                typeof obj.fields.coin_locked_until_epoch === "object" ||
                typeof obj.fields.coin_locked_until_epoch === "function") &&
            (obj.fields.coin_locked_until_epoch.fields !== null &&
                typeof obj.fields.coin_locked_until_epoch.fields === "object" ||
                typeof obj.fields.coin_locked_until_epoch.fields === "function") &&
            obj.fields.coin_locked_until_epoch.fields.vec === "" &&
            isTransactionDigest(obj.fields.coin_locked_until_epoch.type) as boolean) &&
        (isSuiMoveTypeParameterIndex(obj.fields.ending_epoch) as boolean ||
            (obj.fields.ending_epoch !== null &&
                typeof obj.fields.ending_epoch === "object" ||
                typeof obj.fields.ending_epoch === "function") &&
            (obj.fields.ending_epoch.fields !== null &&
                typeof obj.fields.ending_epoch.fields === "object" ||
                typeof obj.fields.ending_epoch.fields === "function") &&
            obj.fields.ending_epoch.fields.vec === "" &&
            isTransactionDigest(obj.fields.ending_epoch.type) as boolean)
    )
}

export function isDelegationSuiObject(obj: any, _argumentName?: string): obj is DelegationSuiObject {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isObjectOwner(obj.owner) as boolean &&
        isTransactionDigest(obj.previousTransaction) as boolean &&
        isSuiMoveTypeParameterIndex(obj.storageRebate) as boolean &&
        isSuiObjectRef(obj.reference) as boolean &&
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isDelegationData(obj.data) as boolean
    )
}

export function isTransferObjectTx(obj: any, _argumentName?: string): obj is TransferObjectTx {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.TransferObject !== null &&
            typeof obj.TransferObject === "object" ||
            typeof obj.TransferObject === "function") &&
        isTransactionDigest(obj.TransferObject.recipient) as boolean &&
        isSuiObjectRef(obj.TransferObject.object_ref) as boolean
    )
}

export function isTransferSuiTx(obj: any, _argumentName?: string): obj is TransferSuiTx {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.TransferSui !== null &&
            typeof obj.TransferSui === "object" ||
            typeof obj.TransferSui === "function") &&
        isTransactionDigest(obj.TransferSui.recipient) as boolean &&
        ((obj.TransferSui.amount !== null &&
            typeof obj.TransferSui.amount === "object" ||
            typeof obj.TransferSui.amount === "function") &&
            isSuiMoveTypeParameterIndex(obj.TransferSui.amount.Some) as boolean ||
            (obj.TransferSui.amount !== null &&
                typeof obj.TransferSui.amount === "object" ||
                typeof obj.TransferSui.amount === "function") &&
            obj.TransferSui.amount.None === null)
    )
}

export function isPayTx(obj: any, _argumentName?: string): obj is PayTx {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.Pay !== null &&
            typeof obj.Pay === "object" ||
            typeof obj.Pay === "function") &&
        Array.isArray(obj.Pay.coins) &&
        obj.Pay.coins.every((e: any) =>
            isSuiObjectRef(e) as boolean
        ) &&
        Array.isArray(obj.Pay.recipients) &&
        obj.Pay.recipients.every((e: any) =>
            isTransactionDigest(e) as boolean
        ) &&
        Array.isArray(obj.Pay.amounts) &&
        obj.Pay.amounts.every((e: any) =>
            isSuiMoveTypeParameterIndex(e) as boolean
        )
    )
}

export function isPublishTx(obj: any, _argumentName?: string): obj is PublishTx {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.Publish !== null &&
            typeof obj.Publish === "object" ||
            typeof obj.Publish === "function") &&
        (obj.Publish.modules !== null &&
            typeof obj.Publish.modules === "object" ||
            typeof obj.Publish.modules === "function") &&
        isSuiMoveTypeParameterIndex(obj.Publish.modules.length) as boolean
    )
}

export function isSharedObjectRef(obj: any, _argumentName?: string): obj is SharedObjectRef {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.objectId) as boolean &&
        isSuiMoveTypeParameterIndex(obj.initialSharedVersion) as boolean
    )
}

export function isObjectArg(obj: any, _argumentName?: string): obj is ObjectArg {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isSuiObjectRef(obj.ImmOrOwned) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isSharedObjectRef(obj.Shared) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTransactionDigest(obj.Shared_Deprecated) as boolean)
    )
}

export function isCallArg(obj: any, _argumentName?: string): obj is CallArg {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            (obj.Pure !== null &&
                typeof obj.Pure === "object" ||
                typeof obj.Pure === "function") &&
            isSuiMoveTypeParameterIndex(obj.Pure.length) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isObjectArg(obj.Object) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            (obj.ObjVec !== null &&
                typeof obj.ObjVec === "object" ||
                typeof obj.ObjVec === "function") &&
            isSuiMoveTypeParameterIndex(obj.ObjVec.length) as boolean)
    )
}

export function isStructTag(obj: any, _argumentName?: string): obj is StructTag {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionDigest(obj.address) as boolean &&
        isTransactionDigest(obj.module) as boolean &&
        isTransactionDigest(obj.name) as boolean &&
        Array.isArray(obj.typeParams) &&
        obj.typeParams.every((e: any) =>
            isTypeTag(e) as boolean
        )
    )
}

export function isTypeTag(obj: any, _argumentName?: string): obj is TypeTag {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            obj.bool === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            obj.u8 === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            obj.u64 === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            obj.u128 === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            obj.address === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            obj.signer === null ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isTypeTag(obj.vector) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            isStructTag(obj.struct) as boolean)
    )
}

export function isMoveCallTx(obj: any, _argumentName?: string): obj is MoveCallTx {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (obj.Call !== null &&
            typeof obj.Call === "object" ||
            typeof obj.Call === "function") &&
        isSuiObjectRef(obj.Call.package) as boolean &&
        isTransactionDigest(obj.Call.module) as boolean &&
        isTransactionDigest(obj.Call.function) as boolean &&
        Array.isArray(obj.Call.typeArguments) &&
        obj.Call.typeArguments.every((e: any) =>
            isTypeTag(e) as boolean
        ) &&
        Array.isArray(obj.Call.arguments) &&
        obj.Call.arguments.every((e: any) =>
            isCallArg(e) as boolean
        )
    )
}

export function isTransaction(obj: any, _argumentName?: string): obj is Transaction {
    return (
        (isTransferObjectTx(obj) as boolean ||
            isTransferSuiTx(obj) as boolean ||
            isPayTx(obj) as boolean ||
            isPublishTx(obj) as boolean ||
            isMoveCallTx(obj) as boolean)
    )
}

export function isTransactionKind(obj: any, _argumentName?: string): obj is TransactionKind {
    return (
        ((obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
            isTransaction(obj.Single) as boolean ||
            (obj !== null &&
                typeof obj === "object" ||
                typeof obj === "function") &&
            Array.isArray(obj.Batch) &&
            obj.Batch.every((e: any) =>
                isTransaction(e) as boolean
            ))
    )
}

export function isTransactionData(obj: any, _argumentName?: string): obj is TransactionData {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        (typeof obj.sender === "undefined" ||
            isTransactionDigest(obj.sender) as boolean) &&
        isSuiMoveTypeParameterIndex(obj.gasBudget) as boolean &&
        isSuiMoveTypeParameterIndex(obj.gasPrice) as boolean &&
        isTransactionKind(obj.kind) as boolean &&
        isSuiObjectRef(obj.gasPayment) as boolean
    )
}

export function isRpcApiVersion(obj: any, _argumentName?: string): obj is RpcApiVersion {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isSuiMoveTypeParameterIndex(obj.major) as boolean &&
        isSuiMoveTypeParameterIndex(obj.minor) as boolean &&
        isSuiMoveTypeParameterIndex(obj.patch) as boolean
    )
}
