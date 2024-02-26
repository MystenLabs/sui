// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class FaucetRateLimitError extends Error {}

type FaucetCoinInfo = {
	amount: number;
	id: string;
	transferTxDigest: string;
};

type FaucetResponse = {
	transferredGasObjects: FaucetCoinInfo[];
	error?: string | null;
};

type BatchFaucetResponse = {
	task?: string | null;
	error?: string | null;
};

type BatchSendStatusType = {
	status: 'INPROGRESS' | 'SUCCEEDED' | 'DISCARDED';
	transferred_gas_objects: { sent: FaucetCoinInfo[] };
};

type BatchStatusFaucetResponse = {
	status: BatchSendStatusType;
	error?: string | null;
};

type FaucetRequest = {
	host: string;
	path: string;
	body?: Record<string, any>;
	headers?: HeadersInit;
	method: 'GET' | 'POST';
};

async function faucetRequest({ host, path, body, headers, method }: FaucetRequest) {
	const endpoint = new URL(path, host).toString();
	const res = await fetch(endpoint, {
		method,
		body: body ? JSON.stringify(body) : undefined,
		headers: {
			'Content-Type': 'application/json',
			...(headers || {}),
		},
	});

	if (res.status === 429) {
		throw new FaucetRateLimitError(
			`Too many requests from this client have been sent to the faucet. Please retry later`,
		);
	}

	try {
		const parsed = await res.json();
		if (parsed.error) {
			throw new Error(`Faucet returns error: ${parsed.error}`);
		}
		return parsed;
	} catch (e) {
		throw new Error(
			`Encountered error when parsing response from faucet, error: ${e}, status ${res.status}, response ${res}`,
		);
	}
}

export async function requestSuiFromFaucetV0(input: {
	host: string;
	recipient: string;
	headers?: HeadersInit;
}): Promise<FaucetResponse> {
	return faucetRequest({
		host: input.host,
		path: '/gas',
		body: {
			FixedAmountRequest: {
				recipient: input.recipient,
			},
		},
		headers: input.headers,
		method: 'POST',
	});
}

export async function requestSuiFromFaucetV1(input: {
	host: string;
	recipient: string;
	headers?: HeadersInit;
}): Promise<BatchFaucetResponse> {
	return faucetRequest({
		host: input.host,
		path: '/v1/gas',
		body: {
			FixedAmountRequest: {
				recipient: input.recipient,
			},
		},
		headers: input.headers,
		method: 'POST',
	});
}

export async function getFaucetRequestStatus(input: {
	host: string;
	taskId: string;
	headers?: HeadersInit;
}): Promise<BatchStatusFaucetResponse> {
	return faucetRequest({
		host: input.host,
		path: `/v1/status/${input.taskId}`,
		headers: input.headers,
		method: 'GET',
	});
}

export function getFaucetHost(network: 'testnet' | 'devnet' | 'localnet') {
	switch (network) {
		case 'testnet':
			return 'https://faucet.testnet.sui.io';
		case 'devnet':
			return 'https://faucet.devnet.sui.io';
		case 'localnet':
			return 'http://127.0.0.1:9123';
		default:
			throw new Error(`Unknown network: ${network}`);
	}
}
