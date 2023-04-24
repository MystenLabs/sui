// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Tabs from '../Tabs';
import { Window } from '../Window';
import { type Connections } from '../connections';
import { type ContentScriptConnection } from '../connections/ContentScriptConnection';
import {
    createPendingRequest,
    getAllPendingRequests,
    getPendingRequest,
    storePendingRequests,
    updatePendingRequest,
} from './storage';
import { type QredoConnectPendingRequest } from './types';
import { qredoConnectPageUrl, validateInputOrThrow } from './utils';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { type Message } from '_src/shared/messaging/messages';

export async function requestUserApproval(
    input: QredoConnectInput,
    connection: ContentScriptConnection,
    message: Message
) {
    const origin = connection.origin;
    const { service, apiUrl, token } = validateInputOrThrow(input);
    const existingPendingRequest = await getPendingRequest({
        service,
        apiUrl,
        origin,
    });
    if (existingPendingRequest) {
        const qredoConnectUrl = qredoConnectPageUrl(existingPendingRequest.id);
        const changes: Parameters<typeof updatePendingRequest>['1'] = {
            messageID: message.id,
            append: true,
            token: token,
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
