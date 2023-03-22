// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SignedMessage } from "@mysten/sui.js";
import type { WalletAccount } from "@wallet-standard/core";

/** The latest API version of the signMessage API. */
export type SuiSignMessageVersion = "1.0.0";

/**
 * A Wallet Standard feature for signing a personal message, and returning the
 * message bytes that were signed, and message signature.
 */
export type SuiSignMessageFeature = {
  /** Namespace for the feature. */
  "sui:signMessage": {
    /** Version of the feature API. */
    version: SuiSignMessageVersion;
    signMessage: SuiSignMessageMethod;
  };
};

export type SuiSignMessageMethod = (
  input: SuiSignMessageInput
) => Promise<SuiSignMessageOutput>;

/** Input for signing messages. */
export interface SuiSignMessageInput {
  message: Uint8Array;
  account: WalletAccount;
}

/** Output of signing messages. */
export interface SuiSignMessageOutput extends SignedMessage {}
