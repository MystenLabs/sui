// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** The latest API version of the deeplink API. */
export type SuiDeeplinkVersion = "0.0.1";

/**
 * TODO: Docs.
 */
export type SuiDeeplinkFeature = {
  /** Namespace for the feature. */
  "sui:deeplink": {
    /** Version of the feature API. */
    version: SuiDeeplinkVersion;
    deeplink: SuiDeeplinkMethod;
  };
};

export type SuiDeeplinkMethod = (
  input: SuiDeeplinkInput
) => Promise<SuiDeeplinkOutput>;

export type SuiDeeplinkType = "stake";

export interface SuiDeeplinkInput {
  type: SuiDeeplinkType;
}

export interface SuiDeeplinkOutput {}

export interface SuiDeeplinkOptions {}
