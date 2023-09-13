// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/src/client';
import { fetchKiosk, getOwnedKiosks } from '../query/kiosk';
import {
	type FetchKioskOptions,
	KioskData,
	OwnedKiosks,
	Network,
	type KioskClientOptions,
} from '../types';
import { TransferPolicyRule, PERSONAL_KIOSK_RULE_ADDRESS, rules } from '../constants';
import { queryTransferPolicy } from '../query/transfer-policy';

/**
 * A Client that allows you to interact with kiosk.
 * Offers utilities to query kiosk, craft transactions to edit your own kiosk,
 * purchase, manage transfer policies, create new kiosks etc.
 */
export class KioskClient {
	client: SuiClient;
	network: Network;
	rules: TransferPolicyRule[];

	constructor(options: KioskClientOptions) {
		this.client = options.client;
		this.network = options.network;
		this.rules = rules; // add all the default rules.
	}

	/// Querying

	/**
	 * Get an addresses's owned kiosks.
	 * @param address The address for which we want to retrieve the kiosks.
	 * @returns An Object containing all the `kioskOwnerCap` objects as well as the kioskIds.
	 */
	async getOwnedKiosks({ address }: { address: string }): Promise<OwnedKiosks> {
		const personalPackageId = PERSONAL_KIOSK_RULE_ADDRESS[this.network];

		return getOwnedKiosks(this.client, address, {
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
	async getKiosk({ id, options }: { id: string; options: FetchKioskOptions }): Promise<KioskData> {
		return (
			await fetchKiosk(
				this.client,
				id,
				{
					limit: 1000,
				},
				options,
			)
		).data;
	}

	/**
	 * Query the Transfer Policy(ies) for type `T`.
	 * @param type The Type we're querying for (E.g `0xMyAddress::hero::Hero`)
	 */
	async getTransferPolicies({ type }: { type: string }) {
		return queryTransferPolicy(this.client, type);
	}

	// Someone would just have to create a `kiosk-client.ts` file in their project, initialize a KioskClient
	// and call the `addRuleResolver` function. Each rule has a `resolve` function.
	// The resolve function is automatically called on `purchaseAndResolve` function call.
	addRuleResolver(rule: TransferPolicyRule) {
		if (this.rules.find((x) => x.rule === rule.rule))
			throw new Error(`Rule ${rule.rule} resolver already exists.`);
		this.rules.push(rule);
	}
}
