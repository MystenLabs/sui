// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { injectDappInterface } from './interface-inject';
import { init as keepAliveInit } from './keep-bg-alive';
import { setupMessagesProxy } from './messages-proxy';

injectDappInterface();
setupMessagesProxy();
keepAliveInit();
