// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/* eslint-disable */

/*
 * Generated type guards for "client.ts".
 * WARNING: Do not manually change this file.
 */
import { HttpHeaders, RpcParams, ValidResponse, ErrorResponse } from "./client";

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
        typeof obj.method === "string" &&
        Array.isArray(obj.args)
    )
}

export function isValidResponse(obj: any, _argumentName?: string): obj is ValidResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.jsonrpc === "2.0" &&
        typeof obj.id === "string"
    )
}

export function isErrorResponse(obj: any, _argumentName?: string): obj is ErrorResponse {
    return (
        (obj !== null &&
            typeof obj === "object" ||
            typeof obj === "function") &&
        obj.jsonrpc === "2.0" &&
        typeof obj.id === "string" &&
        (obj.error !== null &&
            typeof obj.error === "object" ||
            typeof obj.error === "function") &&
        typeof obj.error.message === "string"
    )
}
