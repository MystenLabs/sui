// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});

// docs::#extract-payment
async function extractPaymentDetails(digest: string) {
	const txResult = await client.core.getTransaction({
		digest,
		include: { effects: true, balanceChanges: true },
	});

	if (txResult.$kind !== 'Transaction') {
		return { status: 'failed' as const, digest };
	}

	const payments = (txResult.Transaction.balanceChanges ?? [])
		.filter((change) => BigInt(change.amount) > 0n)
		.map((change) => ({
			recipient: change.address,
			amount: change.amount,
			coinType: change.coinType,
		}));

	return { status: 'success' as const, digest, payments };
}
// docs::/#extract-payment

// docs::#reconcile-algorithm
interface LocalPaymentRecord {
	id: string;
	digest: string | null; // null if submission failed
	amount: bigint;
	recipient: string;
	status: 'pending' | 'confirmed' | 'failed';
}

interface Discrepancy {
	type: string;
	record?: LocalPaymentRecord;
	digest?: string;
	action: string;
}

async function reconcile(
	localRecords: LocalPaymentRecord[],
	onchainDigests: Set<string>,
): Promise<Discrepancy[]> {
	const discrepancies: Discrepancy[] = [];

	for (const record of localRecords) {
		if (record.status === 'confirmed' && record.digest && !onchainDigests.has(record.digest)) {
			// Local says confirmed, but not found onchain
			discrepancies.push({
				type: 'missing_onchain',
				record,
				action: 'Verify digest and re-query. Might need to re-submit.',
			});
		}

		if (record.status === 'pending' && record.digest && onchainDigests.has(record.digest)) {
			// Local says pending, but it is confirmed onchain
			discrepancies.push({
				type: 'unacknowledged_confirmation',
				record,
				action: 'Update local status to confirmed.',
			});
		}
	}

	// Check for onchain transactions not in local records (unexpected payments)
	const localDigests = new Set(localRecords.map((r) => r.digest).filter(Boolean));
	for (const digest of onchainDigests) {
		if (!localDigests.has(digest)) {
			discrepancies.push({
				type: 'unknown_transaction',
				digest,
				action: 'Investigate. Might be a duplicate retry or unauthorized transaction.',
			});
		}
	}

	return discrepancies;
}
// docs::/#reconcile-algorithm

export { extractPaymentDetails, reconcile };
