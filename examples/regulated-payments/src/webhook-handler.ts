// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';

const suiClient = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const providerPublicKey = '0xPROVIDER_PUBLIC_KEY';

function verifyWebhookSignature(_body: unknown, _sig: string | null, _key: string): boolean {
	// Provider-specific signature verification.
	return true;
}

const complianceLogger = {
	warn: async (_msg: string, _data: unknown) => {},
};

const ledger = {
	creditUser: async (_userId: string, _amount: string, _coinType: string) => {},
};

// docs::#webhook-handler
async function handleProviderWebhook(req: Request): Promise<Response> {
	// Step 1: Verify webhook signature.
	const signature = req.headers.get('X-Provider-Signature');
	if (!verifyWebhookSignature(req.body, signature, providerPublicKey)) {
		return new Response('Invalid signature', { status: 401 });
	}

	const payload = await req.json();

	// Step 2: Verify onchain state.
	const txDigest = payload.transactionDigest;
	const txResult = await suiClient.core.getTransaction({
		digest: txDigest,
		include: { effects: true },
	});

	if (txResult.$kind !== 'Transaction') {
		await complianceLogger.warn('Transaction failed onchain', payload);
		return new Response('Onchain verification failed', { status: 422 });
	}

	// Verify the recipient received at least the expected amount.
	const { balance } = await suiClient.core.getBalance({
		owner: payload.recipientAddress,
		coinType: payload.coinType,
	});

	if (BigInt(balance.balance) < BigInt(payload.amount)) {
		await complianceLogger.warn('Webhook does not match onchain state', payload);
		return new Response('Onchain verification failed', { status: 422 });
	}

	// Step 3: Update application state.
	await ledger.creditUser(payload.userId, payload.amount, payload.coinType);

	return new Response('OK', { status: 200 });
}
// docs::/#webhook-handler

export { handleProviderWebhook };
