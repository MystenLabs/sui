// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { v4 as uuid } from 'uuid';
import Browser from 'webextension-polyfill';

import Tabs from './Tabs';
import { Window } from './Window';
import { type Connections } from './connections';
import { type ContentScriptConnection } from './connections/ContentScriptConnection';
import {
    setToSessionStorage,
    getFromSessionStorage,
    isSessionStorageSupported,
} from './storage-utils';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { type Message } from '_src/shared/messaging/messages';

const SESSION_STORAGE_KEY = 'qredo-connect-requests';

type QredoConnectRequestIdentity = {
    service: string;
    apiUrl: string;
    origin: string;
};
type QredoConnectPendingRequest = {
    id: string;
    originFavIcon?: string;
    token: string;
    windowID: number | null;
    messageIDs: string[];
} & QredoConnectRequestIdentity;

function qredoConnectPageUrl(requestID: string) {
    return `${Browser.runtime.getURL(
        'ui.html'
    )}#/dapp/qredo-connect/${encodeURIComponent(requestID)}`;
}

function trimString(input: unknown) {
    return (typeof input === 'string' && input.trim()) || '';
}

function validateOrThrow(input: QredoConnectInput) {
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

async function getAllPendingRequests() {
    return (
        (await getFromSessionStorage<QredoConnectPendingRequest[]>(
            SESSION_STORAGE_KEY,
            []
        )) || []
    );
}

async function getPendingRequest(
    requestIdentity: QredoConnectRequestIdentity | string
) {
    if (!isSessionStorageSupported()) {
        throw new Error(
            'Session storage is required. Please update your browser'
        );
    }
    const allPendingRequests = await getAllPendingRequests();
    return (
        allPendingRequests.find(
            (aRequest) =>
                (typeof requestIdentity === 'string' &&
                    aRequest.id === requestIdentity) ||
                (typeof requestIdentity === 'object' &&
                    requestIdentity.apiUrl === aRequest.apiUrl &&
                    requestIdentity.origin === aRequest.origin &&
                    requestIdentity.service === aRequest.service) ||
                false
        ) || null
    );
}

function storePendingRequests(requests: QredoConnectPendingRequest[]) {
    return setToSessionStorage(SESSION_STORAGE_KEY, requests);
}

async function storePendingRequest(request: QredoConnectPendingRequest) {
    if (!isSessionStorageSupported()) {
        throw new Error(
            'Session storage is required. Please update your browser'
        );
    }
    const allPendingRequests = await getAllPendingRequests();
    const existingIndex = allPendingRequests.findIndex(
        (aRequest) => aRequest.id === request.id
    );
    if (existingIndex >= 0) {
        allPendingRequests.splice(existingIndex, 1, request);
    } else {
        allPendingRequests.push(request);
    }
    await storePendingRequests(allPendingRequests);
}

async function createPendingRequest(
    options: Omit<QredoConnectPendingRequest, 'id' | 'messageIDs' | 'windowID'>,
    messageID: string
) {
    const newRequest: QredoConnectPendingRequest = {
        id: uuid(),
        ...options,
        windowID: null,
        messageIDs: [messageID],
    };
    await storePendingRequest(newRequest);
    return newRequest;
}

async function updatePendingRequest(
    id: string,
    change: { windowID?: number; messageID?: string; append?: boolean }
) {
    const request = await getPendingRequest(id);
    if (!request) {
        return;
    }
    if (typeof change.windowID === 'number') {
        request.windowID = change.windowID;
    }
    if (change.messageID) {
        if (change.append) {
            request.messageIDs.push(change.messageID);
        } else {
            request.messageIDs = [change.messageID];
        }
    }
    await storePendingRequest(request);
}

export async function requestUserApproval(
    input: QredoConnectInput,
    connection: ContentScriptConnection,
    message: Message
) {
    const origin = connection.origin;
    const { service, apiUrl, token } = validateOrThrow(input);
    const existingPendingRequest = await getPendingRequest({
        service,
        apiUrl,
        origin,
    });
    if (existingPendingRequest?.token === token) {
        const qredoConnectUrl = qredoConnectPageUrl(existingPendingRequest.id);
        const changes: Parameters<typeof updatePendingRequest>['1'] = {
            messageID: message.id,
            append: true,
        };
        if (
            !(await Tabs.highlight({
                url: qredoConnectUrl,
                windowID: existingPendingRequest.windowID || undefined,
            }))
        ) {
            const approvalWindow = new Window(qredoConnectUrl);
            await approvalWindow.show();
            if (approvalWindow.id) {
                changes.windowID = approvalWindow.id;
            }
        }
        await updatePendingRequest(existingPendingRequest.id, changes);
        return;
    }
    const request = await createPendingRequest(
        {
            service,
            apiUrl,
            token,
            origin,
            originFavIcon: connection.originFavIcon,
        },
        message.id
    );
    const approvalWindow = new Window(qredoConnectPageUrl(request.id));
    await approvalWindow.show();
    if (approvalWindow.id) {
        await updatePendingRequest(request.id, { windowID: approvalWindow.id });
    }
}

export async function handleOnWindowClosed(
    windowID: number,
    connections: Connections
) {
    const allRequests = await getAllPendingRequests();
    const remainingRequests: QredoConnectPendingRequest[] = [];
    allRequests.forEach((aRequest) => {
        if (aRequest.windowID === windowID) {
            aRequest.messageIDs.forEach((aMessageID) => {
                connections.notifyContentScript(
                    {
                        event: 'qredoConnectResult',
                        origin: aRequest.origin,
                        allowed: false,
                    },
                    aMessageID
                );
            });
        } else {
            remainingRequests.push(aRequest);
        }
    });
    await storePendingRequests(remainingRequests);
}
