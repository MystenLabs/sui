// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
  ExecuteTransactionRequestType,
  SuiTransactionResponse,
  SuiTransactionResponseOptions,
} from "@mysten/sui.js";
import type { SuiSignTransactionInput } from "./suiSignTransaction";

/** The latest API version of the signAndExecuteTransaction API. */
export type SuiSignAndExecuteTransactionVersion = "2.0.0";

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
  extends SuiSignTransactionInput {
  /**
   * `WaitForEffectsCert` or `WaitForLocalExecution`, see details in `ExecuteTransactionRequestType`.
   * Defaults to `WaitForLocalExecution` if options.showEffects or options.showEvents is true
   */
  requestType?: ExecuteTransactionRequestType;
  /** specify which fields to return (e.g., transaction, effects, events, etc). By default, only the transaction digest will be returned. */
  options?: SuiTransactionResponseOptions;
}

/** Output of signing and sending transactions. */
export interface SuiSignAndExecuteTransactionOutput
  extends SuiTransactionResponse {}
