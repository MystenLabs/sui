// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for validating and normalizing Sui addresses
 */

const HEX_REGEX = /^(0x)?[0-9a-fA-F]+$/;
const SUI_ADDRESS_LENGTH = 20; // 20 bytes = 40 hex chars

/**
 * Check if a string is a valid hex string
 */
export function isHexString(value: string): boolean {
  return HEX_REGEX.test(value);
}

/**
 * Normalize a Sui address by removing 0x prefix and padding with zeros
 */
export function normalizeSuiAddress(address: string): string {
  let addr = address.toLowerCase();
  if (addr.startsWith('0x')) {
    addr = addr.slice(2);
  }
  return '0x' + addr.padStart(SUI_ADDRESS_LENGTH * 2, '0');
}

/**
 * Validate if a string is a valid Sui address
 */
export function isValidSuiAddress(address: string): boolean {
  if (!isHexString(address)) {
    return false;
  }

  let addr = address;
  if (addr.startsWith('0x')) {
    addr = addr.slice(2);
  }

  // Address must be at most 40 hex characters (20 bytes)
  return addr.length > 0 && addr.length <= SUI_ADDRESS_LENGTH * 2;
}

/**
 * Check if two Sui addresses are equal (normalized comparison)
 */
export function addressesEqual(addr1: string, addr2: string): boolean {
  try {
    return normalizeSuiAddress(addr1) === normalizeSuiAddress(addr2);
  } catch {
    return false;
  }
}

/**
 * Shorten an address for display purposes
 * @param address - The address to shorten
 * @param prefixLength - Number of characters to show at start (default: 6)
 * @param suffixLength - Number of characters to show at end (default: 4)
 */
export function shortenAddress(
  address: string,
  prefixLength: number = 6,
  suffixLength: number = 4
): string {
  const normalized = normalizeSuiAddress(address);
  if (normalized.length <= prefixLength + suffixLength + 2) {
    return normalized;
  }
  return `${normalized.slice(0, prefixLength)}...${normalized.slice(-suffixLength)}`;
}
