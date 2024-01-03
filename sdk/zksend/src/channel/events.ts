// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { literal, object, optional, Output, string, union, url, uuid } from 'valibot';

export type ZkSendRequestType = 'connect' | 'sign-transaction-block';

export const ZkSendRequest = object({
	id: string([uuid()]),
	data: optional(string()),
});

export type ZkSendRequest = Output<typeof ZkSendRequest>;

export const ZkSendReadyResponse = object({
	type: literal('ready'),
});

export const ZkSendRejectResponse = object({
	type: literal('reject'),
});

export const ZkSendResolveResponse = object({
	type: literal('resolve'),
	data: union([object({ address: string() }), object({ bytes: string(), signature: string() })]),
});

export type ZkSendResolveResponse = Output<typeof ZkSendResolveResponse>;

export const ZkSendResponsePayload = union([
	ZkSendReadyResponse,
	ZkSendRejectResponse,
	ZkSendResolveResponse,
]);
export type ZkSendResponsePayload = Output<typeof ZkSendResponsePayload>;

export const ZkSendResponse = object({
	id: string([uuid()]),
	source: literal('zksend-channel'),
	payload: ZkSendResponsePayload,
});

export type ZkSendResponse = Output<typeof ZkSendResponse>;
