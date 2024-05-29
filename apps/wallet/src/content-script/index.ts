// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { domainScanning } from './domain-scanning';
import { injectDappInterface } from './interface-inject';
import { setupMessagesProxy } from './messages-proxy';

injectDappInterface();
setupMessagesProxy();
domainScanning();
