// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { v4 as uuidV4 } from 'uuid';

import type { Payload } from './payloads/Payload';

export type Message = {
	id: string;
	payload: Payload;
};

export function createMessage<MsgPayload extends Payload>(
	payload: MsgPayload,
	id?: string,
): Message {
	return {
		id: id || uuidV4(),
		payload,
	};
}
