/*
 * Generated type guards for "objects.ts".
 * WARNING: Do not manually change this file.
 */
import { isObjectStatus, isObjectRef, isObjectExistsInfo, isObjectNotExistsInfo, isTransactionResponse, isGatewayTxSeqNumber } from "../index.guard";
import { GetObjectInfoResponse, ObjectDigest, ObjectId, SequenceNumber, RawObjectRef } from "./objects";

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
        isGatewayTxSeqNumber(obj[1]) as boolean &&
        isTransactionResponse(obj[2]) as boolean
    )
}
