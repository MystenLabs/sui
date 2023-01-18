// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
    TRPCClientOutgoingMessage,
    TRPCErrorResponse,
    TRPCRequest,
    TRPCResultMessage,
} from '@trpc/server/rpc';

export type TRPCExtensionRequest = {
    trpc: TRPCRequest | TRPCClientOutgoingMessage;
};

export type TRPCExtensionSuccessResponse = {
    trpc: TRPCResultMessage<unknown>;
};

export type TRPCExtensionErrorResponse = {
    trpc: TRPCErrorResponse;
};

export type TRPCExtensionResponse =
    | TRPCExtensionSuccessResponse
    | TRPCExtensionErrorResponse;
