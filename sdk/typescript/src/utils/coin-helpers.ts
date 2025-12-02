// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for working with Sui coins and balances
 */

const MIST_PER_SUI = 1_000_000_000; // 1 SUI = 10^9 MIST

/**
 * Convert MIST to SUI
 */
export function mistToSui(mist: bigint | number | string): number {
  const mistBigInt = typeof mist === 'bigint' ? mist : BigInt(mist);
  return Number(mistBigInt) / MIST_PER_SUI;
}

/**
 * Convert SUI to MIST
 */
export function suiToMist(sui: number): bigint {
  return BigInt(Math.floor(sui * MIST_PER_SUI));
}

/**
 * Format MIST amount to a human-readable SUI string
 */
export function formatSui(mist: bigint | number | string, decimals: number = 4): string {
  const sui = mistToSui(mist);
  return sui.toFixed(decimals);
}

/**
 * Format balance with proper units (SUI/MIST)
 */
export function formatBalance(mist: bigint | number | string, includeUnit: boolean = true): string {
  const mistBigInt = typeof mist === 'bigint' ? mist : BigInt(mist);

  if (mistBigInt < BigInt(MIST_PER_SUI / 1000)) {
    // Less than 0.001 SUI, show in MIST
    return includeUnit ? `${mistBigInt.toString()} MIST` : mistBigInt.toString();
  }

  const sui = mistToSui(mistBigInt);
  const formatted = sui.toFixed(4);
  return includeUnit ? `${formatted} SUI` : formatted;
}

/**
 * Parse a SUI amount string to MIST
 */
export function parseSuiAmount(amount: string): bigint {
  const cleaned = amount.trim().replace(/,/g, '');
  const num = parseFloat(cleaned);

  if (isNaN(num) || num < 0) {
    throw new Error(`Invalid SUI amount: ${amount}`);
  }

  return suiToMist(num);
}

/**
 * Calculate total balance from multiple coin objects
 */
export function sumCoinBalances(balances: Array<bigint | number | string>): bigint {
  return balances.reduce((total, balance) => {
    const balanceBigInt = typeof balance === 'bigint' ? balance : BigInt(balance);
    return total + balanceBigInt;
  }, BigInt(0));
}

/**
 * Check if a balance is sufficient for a transaction
 */
export function hasSufficientBalance(
  available: bigint | number | string,
  required: bigint | number | string
): boolean {
  const availableBigInt = typeof available === 'bigint' ? available : BigInt(available);
  const requiredBigInt = typeof required === 'bigint' ? required : BigInt(required);
  return availableBigInt >= requiredBigInt;
}

/**
 * Calculate percentage of balance
 */
export function calculatePercentage(
  amount: bigint | number | string,
  percentage: number
): bigint {
  const amountBigInt = typeof amount === 'bigint' ? amount : BigInt(amount);
  return (amountBigInt * BigInt(Math.floor(percentage * 100))) / BigInt(10000);
}

/**
 * Sort coin objects by balance (descending)
 */
export function sortCoinsByBalance<T extends { balance: bigint | number | string }>(
  coins: T[]
): T[] {
  return [...coins].sort((a, b) => {
    const aBalance = typeof a.balance === 'bigint' ? a.balance : BigInt(a.balance);
    const bBalance = typeof b.balance === 'bigint' ? b.balance : BigInt(b.balance);
    return Number(bBalance - aBalance);
  });
}
