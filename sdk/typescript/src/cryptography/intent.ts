// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../types/sui-bcs.js';

// See: sui/crates/sui-types/src/intent.rs
export enum AppId {
	Sui = 0,
}

export enum IntentVersion {
	V0 = 0,
}

export enum IntentScope {
	TransactionData = 0,
	TransactionEffects = 1,
	CheckpointSummary = 2,
	PersonalMessage = 3,
}

export type Intent = [IntentScope, IntentVersion, AppId];

function intentWithScope(scope: IntentScope): Intent {
	return [scope, IntentVersion.V0, AppId.Sui];
}

export function messageWithIntent(scope: IntentScope, message: Uint8Array) {
	let serialized_msg = message;
	// Serialize the personal message with BCS vector to match with rust
	// See: `struct PersonalMessage` in sui/crates/sui-types/src/intent.rs
	if (scope === IntentScope.PersonalMessage) {
		serialized_msg = bcs.ser(['vector', 'u8'], message).toBytes();
	}
	const intent = intentWithScope(scope);
	const intentMessage = new Uint8Array(intent.length + serialized_msg.length);
	intentMessage.set(intent);
	intentMessage.set(serialized_msg, intent.length);
	return intentMessage;
}
