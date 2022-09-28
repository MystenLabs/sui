// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { injectDappInterface } from './interface-inject';
import { setupMessagesProxy } from './messages-proxy';

injectDappInterface();
setupMessagesProxy();
