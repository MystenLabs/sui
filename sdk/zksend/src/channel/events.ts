// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { literal, object, optional, Output, string, union, url, uuid } from 'valibot';

export type ZkSendRequestType = 'connect' | 'sign-transaction-block';

export const ZkSendRequest = object({
	id: string([uuid()]),
	origin: string([url()]),
	data: optional(string()),
});

export type ZkSendRequest = Output<typeof ZkSendRequest>;

export const ZkSendRejectResponse = object({
	type: literal('reject'),
});

export const ZkSendConnectResponse = object({
	type: literal('connect'),
	address: string(),
});

export const ZkSendSignTransactionBlockResponse = object({
	type: literal('sign-transaction-block'),
	signature: string(),
	bytes: string(),
});

export const ZkSendResponsePayload = union([
	ZkSendRejectResponse,
	ZkSendConnectResponse,
	ZkSendSignTransactionBlockResponse,
]);
export type ZkSendResponsePayload = Output<typeof ZkSendResponsePayload>;

export type ZkSendResponsePaylodForType<T extends ZkSendResponsePayload['type']> =
	T extends 'connect'
		? Output<typeof ZkSendConnectResponse>
		: T extends 'sign-transaction-block'
		? Output<typeof ZkSendSignTransactionBlockResponse>
		: never;

export const ZkSendResponse = object({
	id: string([uuid()]),
	source: literal('zksend-channel'),
	payload: ZkSendResponsePayload,
});

export type ZkSendResponse = Output<typeof ZkSendResponse>;
