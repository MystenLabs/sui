// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class Constants {
	public static readonly GAS_BUDGET = 6_000_000_000;

	public static readonly SUI_SCALLING = Math.pow(10, 9);

	/// PRIVATE VARIABLES
	private static readonly BUILD_CMD = 'sui move build --dump-bytecode-as-base64';
	// private static readonly SPOT_BUILD_TARGET_PATH = "../sui-spot-amm-modules/";

	public static getBuildCommand() {
		return Constants.BUILD_CMD;
	}
}

export class RPC {
	public static readonly is_mainnet = false;

	/// PRIVATE VARIABLES
	private static readonly localnet = 'http://127.0.0.1:9000';

	public static get() {
		return RPC.localnet;
	}
}
