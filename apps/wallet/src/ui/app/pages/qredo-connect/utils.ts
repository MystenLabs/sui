// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type UIQredoPendingRequest } from '_src/background/qredo/types';

export const QREDO_PENDING_REQUEST_KEY_COMMON = [
    'qredo-connect',
    'pending-request',
] as const;

export const QREDO_CONNECTION_INFO_KEY_COMMON = ['qredo', 'info'] as const;

export function isUntrustedQredoConnect({
    apiUrl,
    origin,
}: UIQredoPendingRequest) {
    try {
        return (
            new URL(origin).protocol !== 'https:' ||
            new URL(apiUrl).protocol !== 'https:'
        );
    } catch (e) {
        return false;
    }
}
