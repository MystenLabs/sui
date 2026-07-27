// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui/client';

const client = new SuiClient({ url: 'https://fullnode.mainnet.sui.io:443' });

// docs::#extract-payment
async function extractPaymentDetails(digest: string) {
	const result = await client.getTransactionBlock({
		digest,
		options: { showBalanceChanges: true, showEffects: true },
	});

	if (result.effects?.status.status !== 'success') {
		return { status: 'failed' as const, digest };
	}

	const payments = (result.balanceChanges ?? [])
		.filter((change) => BigInt(change.amount) > 0n)
		.map((change) => ({
			recipient: typeof change.owner === 'object' && 'AddressOwner' in change.owner
				? change.owner.AddressOwner
				: null,
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
