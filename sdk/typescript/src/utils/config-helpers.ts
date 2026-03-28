// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for SDK configuration management
 */

export type SuiNetwork = 'mainnet' | 'testnet' | 'devnet' | 'localnet';

export interface NetworkConfig {
  name: SuiNetwork;
  rpcUrl: string;
  faucetUrl?: string;
  explorerUrl: string;
}

/**
 * Predefined network configurations
 */
export const NETWORK_CONFIGS: Record<SuiNetwork, NetworkConfig> = {
  mainnet: {
    name: 'mainnet',
    rpcUrl: 'https://fullnode.mainnet.sui.io:443',
    explorerUrl: 'https://explorer.sui.io',
  },
  testnet: {
    name: 'testnet',
    rpcUrl: 'https://fullnode.testnet.sui.io:443',
    faucetUrl: 'https://faucet.testnet.sui.io',
    explorerUrl: 'https://explorer.sui.io/?network=testnet',
  },
  devnet: {
    name: 'devnet',
    rpcUrl: 'https://fullnode.devnet.sui.io:443',
    faucetUrl: 'https://faucet.devnet.sui.io',
    explorerUrl: 'https://explorer.sui.io/?network=devnet',
  },
  localnet: {
    name: 'localnet',
    rpcUrl: 'http://127.0.0.1:9000',
    explorerUrl: 'http://localhost:3000',
  },
};

/**
 * Get network configuration by name
 */
export function getNetworkConfig(network: SuiNetwork): NetworkConfig {
  return NETWORK_CONFIGS[network];
}

/**
 * Get RPC URL for a network
 */
export function getRpcUrl(network: SuiNetwork): string {
  return NETWORK_CONFIGS[network].rpcUrl;
}

/**
 * Get faucet URL for a network (if available)
 */
export function getFaucetUrl(network: SuiNetwork): string | undefined {
  return NETWORK_CONFIGS[network].faucetUrl;
}

/**
 * Get explorer URL for a network
 */
export function getExplorerUrl(network: SuiNetwork): string {
  return NETWORK_CONFIGS[network].explorerUrl;
}

/**
 * Check if network has a faucet
 */
export function hasFaucet(network: SuiNetwork): boolean {
  return NETWORK_CONFIGS[network].faucetUrl !== undefined;
}

/**
 * Detect network from RPC URL
 */
export function detectNetwork(rpcUrl: string): SuiNetwork | null {
  const url = rpcUrl.toLowerCase();

  if (url.includes('mainnet')) return 'mainnet';
  if (url.includes('testnet')) return 'testnet';
  if (url.includes('devnet')) return 'devnet';
  if (url.includes('127.0.0.1') || url.includes('localhost')) return 'localnet';

  return null;
}

/**
 * Validate RPC URL format
 */
export function isValidRpcUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return ['http:', 'https:'].includes(parsed.protocol);
  } catch {
    return false;
  }
}

/**
 * SDK Configuration options
 */
export interface SdkConfig {
  network: SuiNetwork;
  rpcUrl?: string;
  requestTimeout?: number;
  maxRetries?: number;
  debug?: boolean;
}

/**
 * Default SDK configuration
 */
export const DEFAULT_CONFIG: SdkConfig = {
  network: 'devnet',
  requestTimeout: 30000, // 30 seconds
  maxRetries: 3,
  debug: false,
};

/**
 * Merge configurations with defaults
 */
export function mergeConfig(config: Partial<SdkConfig>): SdkConfig {
  return {
    ...DEFAULT_CONFIG,
    ...config,
    rpcUrl: config.rpcUrl || getRpcUrl(config.network || DEFAULT_CONFIG.network),
  };
}

/**
 * Validate SDK configuration
 */
export function validateConfig(config: SdkConfig): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  // Validate network
  if (!['mainnet', 'testnet', 'devnet', 'localnet'].includes(config.network)) {
    errors.push(`Invalid network: ${config.network}`);
  }

  // Validate RPC URL
  if (config.rpcUrl && !isValidRpcUrl(config.rpcUrl)) {
    errors.push(`Invalid RPC URL: ${config.rpcUrl}`);
  }

  // Validate timeout
  if (config.requestTimeout !== undefined && config.requestTimeout <= 0) {
    errors.push('Request timeout must be positive');
  }

  // Validate retries
  if (config.maxRetries !== undefined && config.maxRetries < 0) {
    errors.push('Max retries cannot be negative');
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Environment-based configuration loader
 */
export function loadConfigFromEnv(): Partial<SdkConfig> {
  const config: Partial<SdkConfig> = {};

  // Load from environment variables
  if (process.env.SUI_NETWORK) {
    config.network = process.env.SUI_NETWORK as SuiNetwork;
  }

  if (process.env.SUI_RPC_URL) {
    config.rpcUrl = process.env.SUI_RPC_URL;
  }

  if (process.env.SUI_REQUEST_TIMEOUT) {
    config.requestTimeout = parseInt(process.env.SUI_REQUEST_TIMEOUT, 10);
  }

  if (process.env.SUI_MAX_RETRIES) {
    config.maxRetries = parseInt(process.env.SUI_MAX_RETRIES, 10);
  }

  if (process.env.SUI_DEBUG) {
    config.debug = process.env.SUI_DEBUG === 'true';
  }

  return config;
}

/**
 * Create configuration from environment with fallback to defaults
 */
export function createConfig(overrides?: Partial<SdkConfig>): SdkConfig {
  const envConfig = loadConfigFromEnv();
  return mergeConfig({ ...envConfig, ...overrides });
}
