// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TRPCError } from '@trpc/server';

export function getErrorFromUnknown(cause: unknown): TRPCError {
    if (cause instanceof Error && cause.name === 'TRPCError') {
        return cause as TRPCError;
    }

    let errorCause: Error | undefined = undefined;
    let stack: string | undefined = undefined;

    if (cause instanceof Error) {
        errorCause = cause;
        stack = cause.stack;
    }

    const error = new TRPCError({
        message: 'Internal server error',
        code: 'INTERNAL_SERVER_ERROR',
        cause: errorCause,
    });

    if (stack) {
        error.stack = stack;
    }

    return error;
}
