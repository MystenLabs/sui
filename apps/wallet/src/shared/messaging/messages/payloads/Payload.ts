// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BasePayload } from './BasePayload';
import type { ErrorPayload } from './ErrorPayload';

export type Payload = BasePayload | ErrorPayload;
