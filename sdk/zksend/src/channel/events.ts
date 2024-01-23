// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Output } from 'valibot';
import { literal, object, optional, string, url, uuid, variant } from 'valibot';

export const ZkSendRequestData = variant('type', [
	object({
		type: literal('connect'),
	}),
	object({
		type: literal('sign-transaction-block'),
		data: string('`data` is required'),
		address: string('`address` is required'),
	}),
	object({
		type: literal('sign-personal-message'),
		bytes: string('`bytes` is required'),
		address: string('`address` is required'),
	}),
]);
export type ZkSendRequestData = Output<typeof ZkSendRequestData>;

export const ZkSendRequest = object({
	id: string('`id` is required', [uuid()]),
	origin: string([url('`origin` must be a valid URL')]),
	name: optional(string()),
	payload: ZkSendRequestData,
});

export type ZkSendRequest = Output<typeof ZkSendRequest>;

export const ZkSendResponseData = variant('type', [
	object({
		type: literal('connect'),
		address: string(),
	}),
	object({
		type: literal('sign-transaction-block'),
		bytes: string(),
		signature: string(),
	}),
	object({
		type: literal('sign-personal-message'),
		bytes: string(),
		signature: string(),
	}),
]);
export type ZkSendResponseData = Output<typeof ZkSendResponseData>;

export const ZkSendResponsePayload = variant('type', [
	object({
		type: literal('reject'),
	}),
	object({
		type: literal('resolve'),
		data: ZkSendResponseData,
	}),
]);
export type ZkSendResponsePayload = Output<typeof ZkSendResponsePayload>;

export const ZkSendResponse = object({
	id: string([uuid()]),
	source: literal('zksend-channel'),
	payload: ZkSendResponsePayload,
});
export type ZkSendResponse = Output<typeof ZkSendResponse>;

export type ZkSendRequestTypes = Record<string, any> & {
	[P in ZkSendRequestData as P['type']]: P;
};

export type ZkSendResponseTypes = {
	[P in ZkSendResponseData as P['type']]: P;
};
