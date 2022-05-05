// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * Generated type guards for "client.ts".
 * WARNING: Do not manually change this file.
 */
import { HttpHeaders, RpcParams, ValidResponse, ErrorResponse } from "./client";
import { isTransactionResponse } from "../index.guard";

export function isHttpHeaders(obj: any, _argumentName?: string): obj is HttpHeaders {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function")
    )
}

export function isRpcParams(obj: any, _argumentName?: string): obj is RpcParams {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        isTransactionResponse(obj.method) as boolean &&
        Array.isArray(obj.args)
    )
}

export function isValidResponse(obj: any, _argumentName?: string): obj is ValidResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.jsonrpc === "2.0" &&
        isTransactionResponse(obj.id) as boolean
    )
}

export function isErrorResponse(obj: any, _argumentName?: string): obj is ErrorResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.jsonrpc === "2.0" &&
        isTransactionResponse(obj.id) as boolean &&
        (obj.error !== null &&
            typeof obj.error === "object" ||
            typeof obj.error === "function") &&
        isTransactionResponse(obj.error.message) as boolean
    )
}
