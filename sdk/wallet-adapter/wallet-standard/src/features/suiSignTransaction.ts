// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SignableTransaction, SignedTransaction } from "@mysten/sui.js";

/** The latest API version of the signTransaction API. */
export type SuiSignTransactionVersion = "1.0.0";

/**
 * A Wallet Standard feature for signing a transaction, and returning the
 * serialized transaction and transaction signature.
 */
export type SuiSignTransactionFeature = {
  /** Namespace for the feature. */
  "sui:signTransaction": {
    /** Version of the feature API. */
    version: SuiSignTransactionVersion;
    signTransaction: SuiSignTransactionMethod;
  };
};

export type SuiSignTransactionMethod = (
  input: SuiSignTransactionInput
) => Promise<SuiSignTransactionOutput>;

/** Input for signing transactions. */
export interface SuiSignTransactionInput {
  transaction: SignableTransaction;
  options?: SuiSignTransactionOptions;
}

/** Output of signing transactions. */
export interface SuiSignTransactionOutput extends SignedTransaction {}

/** Options for signing transactions. */
export interface SuiSignTransactionOptions {}
