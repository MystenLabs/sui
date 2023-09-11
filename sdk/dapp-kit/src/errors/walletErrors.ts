// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * An error that is instantiated when someone attempts to connect to a wallet that they're already connected to.
 */
export class WalletAlreadyConnectedError extends Error {}

/**
 * An error that is instantiated when someone attempts to perform an action that requires an active wallet connection.
 */
export class WalletNotConnectedError extends Error {}
