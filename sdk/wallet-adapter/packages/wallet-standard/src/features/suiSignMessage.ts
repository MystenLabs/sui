// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignatureResponse } from "@mysten/sui.js";

/** The latest API version of the signMessage API. */
export type SuiSignMessageVersion = "1.0.0";

/**
 * A Wallet Standard feature for signing a message
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

/** Input for signing messages. */
export interface SuiSignMessageInput extends Uint8Array { }

/** Output of signing messages. */
export interface SuiSignMessageOutput extends SuiSignatureResponse { }

/** Options for signing messages. */
export interface SuiSignMessageOptions { }
