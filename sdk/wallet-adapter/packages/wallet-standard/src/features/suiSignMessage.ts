// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
  Base64DataBuffer,
  SignaturePubkeyPair,
} from "@mysten/sui.js";

/** The latest API version of the signMessage API. */
export type SuiSignMessageVersion = "1.0.0";

/**
 * A Wallet Standard feature for signing a transaction, and submitting it to the
 * network. The wallet is expected to submit the transaction to the network via RPC,
 * and return the transaction response.
 */
export type SuiSignMessageFeature = {
  /** Namespace for the feature. */
  "standard:signMessage": {
    /** Version of the feature API. */
    version: SuiSignMessageVersion;
    signMessage: SuiSignMessageMethod;
  };
};

export type SuiSignMessageMethod = (
  input: SuiSignMessageInput
) => Promise<SuiSignMessageOutput>;

/** Input for signing and sending transactions. */
export interface SuiSignMessageInput extends Base64DataBuffer { }

/** Output of signing and sending transactions. */
export interface SuiSignMessageOutput extends SignaturePubkeyPair { }

/** Options for signing and sending transactions. */
export interface SuiSignMessageOptions { }
