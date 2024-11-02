// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';

import { init as FeedModule } from './tests/generated/feed.js';
import { init as ManagedObjectModule } from './tests/generated/managed.js';
import { init as PolicyModule } from './tests/generated/policy.js';

const PAYWALLRUS_PACKAGE_ID = '0x00000000000000000000000000000000';

export function withoutCodegen(
	publicKey: Uint8Array,
	owner: string,
	title: string,
	description: string,
) {
	const tx = new Transaction();
	const [accessPolicy, accessPolicyCap] = tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'new_policy',
		arguments: [tx.pure.vector('u8', publicKey)],
	});
	const [publishPolicy, publishPolicyCap] = tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'new_policy',
		arguments: [tx.pure.vector('u8', publicKey)],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'authorize',
		arguments: [publishPolicy, publishPolicyCap, tx.pure.address(owner)],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'authorize',
		arguments: [accessPolicy, accessPolicyCap, tx.pure.address(owner)],
	});

	const [feed, feedCap] = tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'feed',
		function: 'create_feed',
		arguments: [
			publishPolicy,
			accessPolicy,
			tx.pure.string(title ?? ''),
			tx.pure.string(description ?? ''),
		],
	});

	const [commentFeed, commentFeedCap] = tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'feed',
		function: 'create_comment_feed',
		arguments: [accessPolicy],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'feed',
		function: 'share',
		arguments: [feed],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'feed',
		function: 'share',
		arguments: [commentFeed],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'share',
		arguments: [accessPolicy],
	});

	tx.moveCall({
		package: PAYWALLRUS_PACKAGE_ID,
		module: 'policy',
		function: 'share',
		arguments: [publishPolicy],
	});

	tx.transferObjects([accessPolicyCap, publishPolicyCap, feedCap, commentFeedCap], owner);

	return tx;
}

export function withCodegen(
	publicKey: number[],
	owner: string,
	title: string,
	description: string,
	price: number,
) {
	const feedModule = FeedModule(PAYWALLRUS_PACKAGE_ID);
	const policyModule = PolicyModule(PAYWALLRUS_PACKAGE_ID);

	const tx = new Transaction();

	const [accessPolicy, accessPolicyCap] = tx.add(
		policyModule.new_policy({
			arguments: [publicKey],
		}),
	);

	const [publishPolicy, publishPolicyCap] = tx.add(
		policyModule.new_policy({
			arguments: [publicKey],
		}),
	);

	tx.add(
		policyModule.authorize({
			arguments: [publishPolicy, publishPolicyCap, owner],
		}),
	);
	tx.add(
		policyModule.authorize({
			arguments: [accessPolicy, accessPolicyCap, owner],
		}),
	);

	const [feed, feedCap] = tx.add(
		feedModule.create_feed({
			arguments: [publishPolicy, accessPolicy, title ?? '', description ?? ''],
		}),
	);

	const [commentFeed, commentFeedCap] = tx.add(
		feedModule.create_comment_feed({
			arguments: [accessPolicy],
		}),
	);

	tx.add(feedModule.share({ arguments: [feed] }));
	tx.add(feedModule.share({ arguments: [commentFeed] }));
	tx.add(policyModule.share({ arguments: [accessPolicy] }));
	tx.add(policyModule.share({ arguments: [publishPolicy] }));
	tx.transferObjects([accessPolicyCap, publishPolicyCap, feedCap, commentFeedCap], owner);

	return tx;
}

const ENOKI_OBJECT_ID = '0x1234567890abcdef';

export function enokiBorrowWithoutCodegen(id: string, type: string) {
	const tx = new Transaction();

	const [parentNft, promise] = tx.moveCall({
		target: `${PAYWALLRUS_PACKAGE_ID}::managed::borrow`,
		arguments: [tx.object(ENOKI_OBJECT_ID), tx.pure.id(id)],
		typeArguments: [type],
	});

	tx.moveCall({
		target: `0x123::some::function`,
		arguments: [parentNft],
	});

	tx.moveCall({
		target: `${PAYWALLRUS_PACKAGE_ID}::managed::put_back`,
		arguments: [tx.object(ENOKI_OBJECT_ID), parentNft, promise],
		typeArguments: [type],
	});
}

export function enokiBorrowWithCodegen(id: string, type: string) {
	const tx = new Transaction();
	const managedObjectModule = ManagedObjectModule(PAYWALLRUS_PACKAGE_ID);

	const [parentNft, promise] = tx.add(
		managedObjectModule.borrow({
			arguments: [ENOKI_OBJECT_ID, id],
			typeArguments: [type],
		}),
	);

	tx.moveCall({
		target: `0x123::some::function`,
		arguments: [parentNft],
	});

	tx.add(
		managedObjectModule.put_back({
			arguments: [ENOKI_OBJECT_ID, parentNft, promise],
			typeArguments: [type],
		}),
	);
}
