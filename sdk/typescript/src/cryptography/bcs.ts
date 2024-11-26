// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { InferBcsInput } from '@mysten/bcs';
import { bcs } from '@mysten/bcs';

export const passkeyAuthenticator = bcs.struct('PasskeyAuthenticator', {
	authenticatorData: bcs.vector(bcs.u8()),
	clientDataJson: bcs.string(),
	userSignature: bcs.vector(bcs.u8()),
});

export type PasskeySignature = InferBcsInput<typeof passkeyAuthenticator>;
