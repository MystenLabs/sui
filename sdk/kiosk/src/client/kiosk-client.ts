// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PaginationArguments, SuiClient } from '@mysten/sui/client';

import {
	FLOOR_PRICE_RULE_ADDRESS,
	getBaseRules,
	KIOSK_LOCK_RULE_ADDRESS,
	PERSONAL_KIOSK_RULE_ADDRESS,
	ROYALTY_RULE_ADDRESS,
	rules,
} from '../constants.js';
import type { BaseRulePackageIds, TransferPolicyRule } from '../constants.js';
import { fetchKiosk, fetchKioskExtension, getOwnedKiosks } from '../query/kiosk.js';
import {
	queryOwnedTransferPolicies,
	queryTransferPolicy,
	queryTransferPolicyCapsByType,
} from '../query/transfer-policy.js';
import { Network } from '../types/index.js';
import type {
	FetchKioskOptions,
	KioskClientOptions,
	KioskData,
	OwnedKiosks,
} from '../types/index.js';

/**
 * A Client that allows you to interact with kiosk.
 * Offers utilities to query kiosk, craft transactions to edit your own kiosk,
 * purchase, manage transfer policies, create new kiosks etc.
 * If you pass packageIds, all functionality will be managed using these packages.
 */
export class KioskClient {
	client: SuiClient;
	network: Network;
	rules: TransferPolicyRule[];
	packageIds?: BaseRulePackageIds;

	constructor(options: KioskClientOptions) {
		this.client = options.client;
		this.network = options.network;
		this.rules = rules; // add all the default rules.
		this.packageIds = options.packageIds;

		// Add the custom Package Ids too on the rule list.
		// Only adds the rules that are passed in the packageId object.
		if (options.packageIds) this.rules.push(...getBaseRules(options.packageIds));
	}

	/// Querying

	/**
	 * Get an addresses's owned kiosks.
	 * @param address The address for which we want to retrieve the kiosks.
	 * @param pagination Optional pagination arguments.
	 * @returns An Object containing all the `kioskOwnerCap` objects as well as the kioskIds.
	 */
	async getOwnedKiosks({
		address,
		pagination,
	}: {
		address: string;
		pagination?: PaginationArguments<string>;
	}): Promise<OwnedKiosks> {
		const personalPackageId =
			this.packageIds?.personalKioskRulePackageId || PERSONAL_KIOSK_RULE_ADDRESS[this.network];

		return getOwnedKiosks(this.client, address, {
			pagination,
			personalKioskType: personalPackageId
				? `${personalPackageId}::personal_kiosk::PersonalKioskCap`
				: '',
		});
	}

	/**
	 * Fetches the kiosk contents.
	 * @param kioskId The ID of the kiosk to fetch.
	 * @param options Optioal
	 * @returns
	 */
	async getKiosk({ id, options }: { id: string; options?: FetchKioskOptions }): Promise<KioskData> {
		return (
			await fetchKiosk(
				this.client,
				id,
				{
					limit: 1000,
				},
				options || {},
			)
		).data;
	}

	/**
	 * Fetch the extension data (if any) for a kiosk, by type
	 * @param kioskId The ID of the kiosk to lookup
	 * @param extensionType The Type of the extension (can be used from by using the type returned by `getKiosk()`)
	 */
	async getKioskExtension({ kioskId, type }: { kioskId: string; type: string }) {
		return fetchKioskExtension(this.client, kioskId, type);
	}

	/**
	 * Query the Transfer Policy(ies) for type `T`.
	 * @param type The Type we're querying for (E.g `0xMyAddress::hero::Hero`)
	 */
	async getTransferPolicies({ type }: { type: string }) {
		return queryTransferPolicy(this.client, type);
	}

	/**
	 * Query all the owned transfer policies for an address.
	 * Returns `TransferPolicyCap` which uncludes `policyId, policyCapId, type`.
	 * @param address The address we're searching the owned transfer policies for.
	 */
	async getOwnedTransferPolicies({ address }: { address: string }) {
		return queryOwnedTransferPolicies(this.client, address);
	}

	/**
	 * Query the Transfer Policy Cap for type `T`, owned by `address`
	 * @param type The Type `T` for the object
	 * @param address The address that owns the cap.
	 */
	async getOwnedTransferPoliciesByType({ type, address }: { type: string; address: string }) {
		return queryTransferPolicyCapsByType(this.client, address, type);
	}

	// Someone would just have to create a `kiosk-client.ts` file in their project, initialize a KioskClient
	// and call the `addRuleResolver` function. Each rule has a `resolve` function.
	// The resolve function is automatically called on `purchaseAndResolve` function call.
	addRuleResolver(rule: TransferPolicyRule) {
		if (this.rules.find((x) => x.rule === rule.rule))
			throw new Error(`Rule ${rule.rule} resolver already exists.`);
		this.rules.push(rule);
	}

	/**
	 * A convenient helper to get the packageIds for our supported ruleset,
	 * based on `kioskClient` configuration.
	 */
	getRulePackageId(
		rule:
			| 'kioskLockRulePackageId'
			| 'royaltyRulePackageId'
			| 'personalKioskRulePackageId'
			| 'floorPriceRulePackageId',
	) {
		const rules = this.packageIds || {};
		const network = this.network;

		/// Check existence of rule based on network and throw an error if it's not found.
		/// We always have a fallback for testnet or mainnet.
		if (!rules[rule] && network !== Network.MAINNET && network !== Network.TESTNET) {
			throw new Error(`Missing packageId for rule ${rule}`);
		}

		switch (rule) {
			case 'kioskLockRulePackageId':
				return rules[rule] || KIOSK_LOCK_RULE_ADDRESS[network];
			case 'royaltyRulePackageId':
				return rules[rule] || ROYALTY_RULE_ADDRESS[network];
			case 'personalKioskRulePackageId':
				return rules[rule] || PERSONAL_KIOSK_RULE_ADDRESS[network];
			case 'floorPriceRulePackageId':
				return rules[rule] || FLOOR_PRICE_RULE_ADDRESS[network];
		}
	}
}
