// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
  SignableTransaction,
  SuiTransactionResponse,
} from "@mysten/sui.js";
import type { SignAndSendTransactionInput } from "@wallet-standard/features";

/** The latest API version of the signAndExecuteTransaction API. */
export type SuiSignAndExecuteTransactionVersion = "1.0.0";

/**
 * A Wallet Standard feature for signing a transaction, and submitting it to the
 * network. The wallet is expected to submit the transaction to the network via RPC,
 * and return the transaction response.
 */
export type SuiSignAndExecuteTransactionFeature = {
  /** Namespace for the feature. */
  "sui:signAndExecuteTransaction": {
    /** Version of the feature API. */
    version: SuiSignAndExecuteTransactionVersion;
    signAndExecuteTransaction: SuiSignAndExecuteTransactionMethod;
  };
};

export type SuiSignAndExecuteTransactionMethod = (
  input: SuiSignAndExecuteTransactionInput
) => Promise<SuiSignAndExecuteTransactionOutput>;

/** Input for signing and sending transactions. */
export interface SuiSignAndExecuteTransactionInput
  extends Omit<
    SignAndSendTransactionInput,
    // TODO: Right now, we don't have intent signing, but eventually we'll need to re-introduce
    // the concept of chains + account during the signing here.
    "transaction" | "chain" | "account"
  > {
  options?: SuiSignAndExecuteTransactionOptions;
  transaction: SignableTransaction;
}

/** Output of signing and sending transactions. */
export interface SuiSignAndExecuteTransactionOutput
  extends SuiTransactionResponse {}

/** Options for signing and sending transactions. */
export interface SuiSignAndExecuteTransactionOptions {}
