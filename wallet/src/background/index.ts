// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { Connections } from './connections';
import { openInNewTab } from '_shared/utils';

Browser.runtime.onInstalled.addListener((details) => {
    if (details.reason === 'install') {
        openInNewTab();
    }
});

const messaging = new Connections();

// Browser.runtime.onConnect.addListener((port) => {
//     switch (port.name) {
//         case CONTENT_TO_BACKGROUND_CHANNEL_NAME:
//             console.log(`New connection from content script`, port);
//             break;
//         case UI_TO_BACKGROUND_CHANNEL_NAME:
//             console.log(`New connection from ui`, port);
//             break;
//         default:
//             console.log(`Unknown connection ${port.name}, disconnecting`);
//             port.disconnect();
//             break;
//     }
//     // const origin = port.sender?.origin;
//     // if (origin) {
//     //     console.log('bg script new connection', origin);
//     //     port.onMessage.addListener(async (msg) => {
//     //         console.log(`Message from ${msg[MSG_SENDER_FIELD]}`, msg);
//     //         if (port.name === CONTENT_TO_BACKGROUND_CHANNEL_NAME) {
//     //             const newWindow = await openPermissionWindow('PERMISSION_ID');
//     //             console.log('new window opened', newWindow);
//     //         } else if (port.name === UI_TO_BACKGROUND_CHANNEL_NAME) {
//     //             if (msg.payload.type === 'get-permission-requests') {
//     //                 const response: Message<PermissionRequests> = {
//     //                     id: 'aaaa',
//     //                     sender: 'background-script',
//     //                     responseForID: msg.id,
//     //                     payload: {
//     //                         type: 'permission-request',
//     //                         permissions: [
//     //                             {
//     //                                 id: 'PERMISSION_ID',
//     //                                 accounts: [],
//     //                                 allowed: null,
//     //                                 createdDate: new Date().toISOString(),
//     //                                 origin,
//     //                                 permissions: ['viewAccount'],
//     //                                 responseDate: null,
//     //                             },
//     //                         ],
//     //                     },
//     //                 };
//     //                 port.postMessage(response);
//     //             }
//     //         }
//     //     });
//     //     port.onDisconnect.addListener(() => {
//     //         const index = connectedPorts.indexOf(port);
//     //         console.log('BG script Port disconnected', { port, index });
//     //         if (index >= 0) {
//     //             connectedPorts.splice(index, 1);
//     //             console.log('BG script Port removed', { connectedPorts });
//     //         }
//     //     });
//     //     connectedPorts.push(port);
//     // } else {
//     //     console.log('Origin not found. Disconnecting port', port);
//     //     port.disconnect();
//     // }
// });
