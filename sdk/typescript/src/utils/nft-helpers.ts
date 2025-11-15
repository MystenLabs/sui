// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for working with Sui NFTs
 */

export interface NftMetadata {
  name?: string;
  description?: string;
  image?: string;
  external_url?: string;
  attributes?: Array<{
    trait_type: string;
    value: string | number;
  }>;
}

/**
 * Extract NFT metadata from object fields
 */
export function extractNftMetadata(fields: Record<string, any>): NftMetadata {
  const metadata: NftMetadata = {};

  if (fields.name) {
    metadata.name = String(fields.name);
  }

  if (fields.description) {
    metadata.description = String(fields.description);
  }

  if (fields.url || fields.image_url || fields.image) {
    metadata.image = String(fields.url || fields.image_url || fields.image);
  }

  if (fields.link || fields.external_url) {
    metadata.external_url = String(fields.link || fields.external_url);
  }

  // Extract attributes if present
  if (fields.attributes && Array.isArray(fields.attributes)) {
    metadata.attributes = fields.attributes.map((attr: any) => ({
      trait_type: String(attr.trait_type || attr.key || ''),
      value: attr.value,
    }));
  }

  return metadata;
}

/**
 * Check if an object appears to be an NFT based on its fields
 */
export function isNftLike(fields: Record<string, any>): boolean {
  // Common NFT field patterns
  const hasName = 'name' in fields;
  const hasImage = 'url' in fields || 'image_url' in fields || 'image' in fields;
  const hasId = 'id' in fields;

  return hasId && (hasName || hasImage);
}

/**
 * Generate NFT display name from metadata
 */
export function getNftDisplayName(metadata: NftMetadata, objectId?: string): string {
  if (metadata.name) {
    return metadata.name;
  }

  if (objectId) {
    return `NFT ${objectId.slice(0, 8)}...`;
  }

  return 'Unnamed NFT';
}

/**
 * Format NFT attributes for display
 */
export function formatNftAttributes(
  attributes: Array<{ trait_type: string; value: string | number }>
): string {
  return attributes.map((attr) => `${attr.trait_type}: ${attr.value}`).join(', ');
}

/**
 * Check if image URL is valid
 */
export function isValidImageUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return ['http:', 'https:', 'ipfs:', 'data:'].includes(parsed.protocol);
  } catch {
    return false;
  }
}

/**
 * Convert IPFS URL to HTTP gateway URL
 */
export function ipfsToHttp(url: string, gateway: string = 'https://ipfs.io'): string {
  if (!url) return url;

  // Handle ipfs:// protocol
  if (url.startsWith('ipfs://')) {
    const hash = url.replace('ipfs://', '');
    return `${gateway}/ipfs/${hash}`;
  }

  // Handle /ipfs/ paths
  if (url.startsWith('/ipfs/')) {
    return `${gateway}${url}`;
  }

  return url;
}

/**
 * Group NFTs by collection (based on type)
 */
export function groupNftsByCollection<T extends { type: string }>(
  nfts: T[]
): Map<string, T[]> {
  const collections = new Map<string, T[]>();

  for (const nft of nfts) {
    const existing = collections.get(nft.type) || [];
    existing.push(nft);
    collections.set(nft.type, existing);
  }

  return collections;
}

/**
 * Filter NFTs by attribute
 */
export function filterNftsByAttribute(
  nfts: Array<{ metadata: NftMetadata }>,
  traitType: string,
  value?: string | number
): Array<{ metadata: NftMetadata }> {
  return nfts.filter((nft) => {
    const attributes = nft.metadata.attributes || [];
    return attributes.some(
      (attr) =>
        attr.trait_type === traitType && (value === undefined || attr.value === value)
    );
  });
}

/**
 * Sort NFTs by name
 */
export function sortNftsByName<T extends { metadata: NftMetadata }>(
  nfts: T[],
  ascending: boolean = true
): T[] {
  return [...nfts].sort((a, b) => {
    const nameA = (a.metadata.name || '').toLowerCase();
    const nameB = (b.metadata.name || '').toLowerCase();

    if (ascending) {
      return nameA.localeCompare(nameB);
    } else {
      return nameB.localeCompare(nameA);
    }
  });
}
