// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GetCoinsParams, SuiClient } from '@mysten/sui/client';
import { getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui/faucet';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

import { RPC } from './constants';

function base64ToUint8Array(base64: string) {
	var binaryString = atob(base64);
	var bytes = new Uint8Array(binaryString.length);
	for (var i = 0; i < binaryString.length; i++) {
		bytes[i] = binaryString.charCodeAt(i);
	}
	return bytes;
}

export class Utils {
	public static provider: SuiClient = new SuiClient({ url: RPC.get() });

	public static async getCoin(pubkey: string) {
		const params: GetCoinsParams = {
			owner: pubkey,
		};
		let res = await this.provider.getCoins(params);
		let obId = res['data'][0]['coinObjectId'];
		return obId;
	}

	public static async getFaucet(pubkey: string) {
		return requestSuiFromFaucetV0({
			host: getFaucetHost('localnet'),
			recipient: pubkey,
		});
	}

	public static async getDeployer(): Promise<Ed25519Keypair> {
		return Ed25519Keypair.fromSecretKey(
			base64ToUint8Array('0000000000000000000000000000000000000000000'),
		);
	}

	public static async getSigner(): Promise<Ed25519Keypair> {
		return new Ed25519Keypair();
	}
}