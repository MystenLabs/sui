// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WindowMessageStream } from '_messaging/WindowMessageStream';

export function setupMessagesProxy() {
    const windowMsgStream = new WindowMessageStream(
        'sui_content-script',
        'sui_in-page'
    );
    windowMsgStream.messages.subscribe((msg) => {
        // TODO implement
        // eslint-disable-next-line no-console
        console.log('[ContentScriptProxy] message from inPage', msg);
    });
}
