// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for the Sui TypeScript SDK
 *
 * This module exports all utility functions for convenient import.
 */

// Address utilities
export {
  isHexString,
  normalizeSuiAddress,
  isValidSuiAddress,
  addressesEqual,
  shortenAddress,
} from './address-validator';

// Coin utilities
export {
  mistToSui,
  suiToMist,
  formatSui,
  formatBalance,
  parseSuiAmount,
  sumCoinBalances,
  hasSufficientBalance,
  calculatePercentage,
  sortCoinsByBalance,
} from './coin-helpers';

// NFT utilities
export {
  extractNftMetadata,
  isNftLike,
  getNftDisplayName,
  formatNftAttributes,
  isValidImageUrl,
  ipfsToHttp,
  groupNftsByCollection,
  filterNftsByAttribute,
  sortNftsByName,
} from './nft-helpers';

export type { NftMetadata } from './nft-helpers';

// Transaction utilities
export {
  extractTransactionSummary,
  isTransactionSuccessful,
  getTransactionError,
  calculateGasCost,
  getCreatedObjectIds,
  getMutatedObjectIds,
  formatDigest,
  getExplorerUrl,
  parseTransactionTimestamp,
  groupTransactionsByStatus,
  calculateAverageGasCost,
  didTransactionModifyObject,
} from './transaction-helpers';

export type { TransactionSummary } from './transaction-helpers';

// Event utilities
export {
  filterEvents,
  parseEventType,
  groupEventsByType,
  groupEventsBySender,
  sortEventsByTimestamp,
  getUniqueEventTypes,
  getUniqueSenders,
  calculateEventFrequency,
  getEventsInTimeRange,
  getLatestEvents,
  eventMatchesPattern,
  extractEventField,
} from './event-helpers';

export type { EventFilter } from './event-helpers';

// Properties utilities
export { defineReadOnly } from './properties';
