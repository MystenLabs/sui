// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Connection } from './Connection';
import { createMessage } from '_messages';
import {
    isGetPermissionRequests,
    isPermissionResponse,
} from '_payloads/permissions';
import Permissions from '_src/background/Permissions';

import type { Message } from '_messages';
import type { PortChannelName } from '_messaging/PortChannelName';
import type { Permission, PermissionRequests } from '_payloads/permissions';

export class UiConnection extends Connection {
    public static readonly CHANNEL: PortChannelName = 'sui_ui<->background';

    protected async handleMessage(msg: Message) {
        const { payload, id } = msg;
        if (isGetPermissionRequests(payload)) {
            this.sendPermissions(
                Object.values(await Permissions.getPermissions()),
                id
            );
        } else if (isPermissionResponse(payload)) {
            Permissions.handlePermissionResponse(payload);
        }
    }

    private sendPermissions(permissions: Permission[], requestID: string) {
        this.send(
            createMessage<PermissionRequests>(
                {
                    type: 'permission-request',
                    permissions,
                },
                requestID
            )
        );
    }
}
