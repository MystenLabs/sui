// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PortStream } from '_messaging/PortStream';
import { WindowMessageStream } from '_messaging/WindowMessageStream';

function createPort(windowMsgStream: WindowMessageStream) {
    const port = PortStream.connectToBackgroundService(
        'sui_content<->background'
    );
    port.onMessage.subscribe((msg) => {
        windowMsgStream.send(msg);
    });
    const windowMsgSub = windowMsgStream.messages.subscribe((msg) => {
        port.sendMessage(msg);
    });
    port.onDisconnect.subscribe((port) => {
        windowMsgSub.unsubscribe();
        createPort(windowMsgStream);
    });
}

export function setupMessagesProxy() {
    const windowMsgStream = new WindowMessageStream(
        'sui_content-script',
        'sui_in-page'
    );
    createPort(windowMsgStream);
}
