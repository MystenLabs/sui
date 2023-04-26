// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';

export function qredoConnectPageUrl(requestID: string) {
    return `${Browser.runtime.getURL(
        'ui.html'
    )}#/dapp/qredo-connect/${encodeURIComponent(requestID)}`;
}

export function trimString(input: unknown) {
    return (typeof input === 'string' && input.trim()) || '';
}

export function validateInputOrThrow(input: QredoConnectInput) {
    if (!input) {
        throw new Error('Invalid Qredo connect parameters');
    }
    let apiUrl;
    try {
        apiUrl = new URL(input.apiUrl);
        if (!['http:', 'https:'].includes(apiUrl.protocol)) {
            throw new Error('Only https (or http) is supported');
        }
    } catch (e) {
        throw new Error(`Not valid apiUrl. ${(e as Error).message}`);
    }
    const service = trimString(input.service);
    if (!service) {
        throw new Error('Invalid service name');
    }
    const token = trimString(input.token);
    if (!token) {
        throw new Error('Invalid token');
    }
    return {
        service,
        apiUrl: apiUrl.toString(),
        token,
    };
}
