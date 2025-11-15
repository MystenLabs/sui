// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for working with Sui transactions
 */

export interface TransactionSummary {
  status: 'success' | 'failure';
  gasUsed: bigint;
  gasCost: bigint;
  timestamp?: number;
  digest: string;
  changes?: {
    created: number;
    mutated: number;
    deleted: number;
    transferred: number;
  };
}

/**
 * Extract transaction summary from effects
 */
export function extractTransactionSummary(effects: any): TransactionSummary {
  const summary: TransactionSummary = {
    status: effects.status?.status === 'success' ? 'success' : 'failure',
    gasUsed: BigInt(effects.gasUsed?.computationCost || 0),
    gasCost: BigInt(effects.gasUsed?.storageCost || 0) + BigInt(effects.gasUsed?.computationCost || 0),
    digest: effects.transactionDigest || '',
  };

  // Count object changes
  if (effects.created || effects.mutated || effects.deleted || effects.wrapped) {
    summary.changes = {
      created: effects.created?.length || 0,
      mutated: effects.mutated?.length || 0,
      deleted: (effects.deleted?.length || 0) + (effects.wrapped?.length || 0),
      transferred: effects.transferred?.length || 0,
    };
  }

  return summary;
}

/**
 * Check if transaction was successful
 */
export function isTransactionSuccessful(effects: any): boolean {
  return effects?.status?.status === 'success';
}

/**
 * Get error message from failed transaction
 */
export function getTransactionError(effects: any): string | undefined {
  if (effects?.status?.status === 'failure') {
    return effects.status.error || 'Unknown error';
  }
  return undefined;
}

/**
 * Calculate total gas cost in MIST
 */
export function calculateGasCost(effects: any): bigint {
  const computationCost = BigInt(effects.gasUsed?.computationCost || 0);
  const storageCost = BigInt(effects.gasUsed?.storageCost || 0);
  const storageRebate = BigInt(effects.gasUsed?.storageRebate || 0);

  return computationCost + storageCost - storageRebate;
}

/**
 * Extract created object IDs from transaction effects
 */
export function getCreatedObjectIds(effects: any): string[] {
  if (!effects.created) return [];

  return effects.created.map((obj: any) => {
    if (obj.reference?.objectId) {
      return obj.reference.objectId;
    }
    if (obj.objectId) {
      return obj.objectId;
    }
    return null;
  }).filter(Boolean);
}

/**
 * Extract mutated object IDs from transaction effects
 */
export function getMutatedObjectIds(effects: any): string[] {
  if (!effects.mutated) return [];

  return effects.mutated.map((obj: any) => {
    if (obj.reference?.objectId) {
      return obj.reference.objectId;
    }
    if (obj.objectId) {
      return obj.objectId;
    }
    return null;
  }).filter(Boolean);
}

/**
 * Format transaction digest for display
 */
export function formatDigest(digest: string, length: number = 16): string {
  if (digest.length <= length) return digest;
  const halfLength = Math.floor(length / 2);
  return `${digest.slice(0, halfLength)}...${digest.slice(-halfLength)}`;
}

/**
 * Get transaction URL for Sui Explorer
 */
export function getExplorerUrl(
  digest: string,
  network: 'mainnet' | 'testnet' | 'devnet' = 'devnet'
): string {
  const baseUrl = network === 'mainnet'
    ? 'https://explorer.sui.io'
    : `https://explorer.sui.io/?network=${network}`;
  return `${baseUrl}/transaction/${digest}`;
}

/**
 * Parse transaction timestamp
 */
export function parseTransactionTimestamp(timestamp: number | string): Date {
  const ts = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
  return new Date(ts);
}

/**
 * Group transactions by status
 */
export function groupTransactionsByStatus<T extends { status: string }>(
  transactions: T[]
): {
  successful: T[];
  failed: T[];
} {
  const successful: T[] = [];
  const failed: T[] = [];

  for (const tx of transactions) {
    if (tx.status === 'success') {
      successful.push(tx);
    } else {
      failed.push(tx);
    }
  }

  return { successful, failed };
}

/**
 * Calculate average gas cost from multiple transactions
 */
export function calculateAverageGasCost(effects: any[]): bigint {
  if (effects.length === 0) return BigInt(0);

  const total = effects.reduce(
    (sum, effect) => sum + calculateGasCost(effect),
    BigInt(0)
  );

  return total / BigInt(effects.length);
}

/**
 * Check if transaction modified specific object
 */
export function didTransactionModifyObject(effects: any, objectId: string): boolean {
  const created = getCreatedObjectIds(effects);
  const mutated = getMutatedObjectIds(effects);

  return created.includes(objectId) || mutated.includes(objectId);
}
