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

type BatchSendStatusType = 'INPROGRESS' | 'SUCCEEDED' | 'DISCARDED';

type BatchStatusFaucetResponse = {
	status: BatchSendStatusType;
	error?: string | null;
};

async function faucetRequest(
	host: string,
	path: string,
	body: Record<string, any>,
	headers?: HeadersInit,
) {
	const endpoint = new URL(path, host).toString();
	const res = await fetch(endpoint, {
		method: 'POST',
		body: JSON.stringify(body),
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
	return faucetRequest(
		input.host,
		'/gas',
		{
			FixedAmountRequest: {
				recipient: input.recipient,
			},
		},
		input.headers,
	);
}

export async function requestSuiFromFaucetV1(input: {
	host: string;
	recipient: string;
	headers?: HeadersInit;
}): Promise<BatchFaucetResponse> {
	return faucetRequest(
		input.host,
		'/v1/gas',
		{
			FixedAmountRequest: {
				recipient: input.recipient,
			},
		},
		input.headers,
	);
}

export async function getFaucetRequestStatus(input: {
	host: string;
	taskId: string;
	headers?: HeadersInit;
}): Promise<BatchStatusFaucetResponse> {
	return faucetRequest(
		input.host,
		'/v1/status',
		{
			task_id: {
				task_id: input.taskId,
			},
		},
		input.headers,
	);
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
