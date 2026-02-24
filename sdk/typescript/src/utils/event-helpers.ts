// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility functions for working with Sui events
 */

export interface EventFilter {
  eventType?: string;
  sender?: string;
  packageId?: string;
  moduleName?: string;
  fromTimestamp?: number;
  toTimestamp?: number;
}

/**
 * Filter events based on criteria
 */
export function filterEvents<T extends { type: string; sender?: string; timestamp?: number }>(
  events: T[],
  filter: EventFilter
): T[] {
  return events.filter((event) => {
    // Filter by event type
    if (filter.eventType && !event.type.includes(filter.eventType)) {
      return false;
    }

    // Filter by sender
    if (filter.sender && event.sender !== filter.sender) {
      return false;
    }

    // Filter by package ID
    if (filter.packageId && !event.type.startsWith(filter.packageId)) {
      return false;
    }

    // Filter by module name
    if (filter.moduleName && !event.type.includes(`::${filter.moduleName}::`)) {
      return false;
    }

    // Filter by timestamp range
    if (event.timestamp) {
      if (filter.fromTimestamp && event.timestamp < filter.fromTimestamp) {
        return false;
      }
      if (filter.toTimestamp && event.timestamp > filter.toTimestamp) {
        return false;
      }
    }

    return true;
  });
}

/**
 * Parse event type to extract package, module, and event name
 */
export function parseEventType(eventType: string): {
  packageId: string;
  moduleName: string;
  eventName: string;
} | null {
  // Expected format: 0x2::coin::CoinCreated<0x2::sui::SUI>
  const match = eventType.match(/^(0x[a-fA-F0-9]+)::([^:]+)::([^<]+)/);

  if (!match) {
    return null;
  }

  return {
    packageId: match[1],
    moduleName: match[2],
    eventName: match[3],
  };
}

/**
 * Group events by type
 */
export function groupEventsByType<T extends { type: string }>(
  events: T[]
): Map<string, T[]> {
  const grouped = new Map<string, T[]>();

  for (const event of events) {
    const existing = grouped.get(event.type) || [];
    existing.push(event);
    grouped.set(event.type, existing);
  }

  return grouped;
}

/**
 * Group events by sender
 */
export function groupEventsBySender<T extends { sender: string }>(
  events: T[]
): Map<string, T[]> {
  const grouped = new Map<string, T[]>();

  for (const event of events) {
    const existing = grouped.get(event.sender) || [];
    existing.push(event);
    grouped.set(event.sender, existing);
  }

  return grouped;
}

/**
 * Sort events by timestamp
 */
export function sortEventsByTimestamp<T extends { timestamp?: number }>(
  events: T[],
  ascending: boolean = true
): T[] {
  return [...events].sort((a, b) => {
    const tsA = a.timestamp || 0;
    const tsB = b.timestamp || 0;

    return ascending ? tsA - tsB : tsB - tsA;
  });
}

/**
 * Get unique event types from event list
 */
export function getUniqueEventTypes<T extends { type: string }>(events: T[]): string[] {
  return Array.from(new Set(events.map((e) => e.type)));
}

/**
 * Get unique senders from event list
 */
export function getUniqueSenders<T extends { sender: string }>(events: T[]): string[] {
  return Array.from(new Set(events.map((e) => e.sender)));
}

/**
 * Calculate event frequency (events per time period)
 */
export function calculateEventFrequency<T extends { timestamp: number }>(
  events: T[],
  periodMs: number = 60000 // Default: 1 minute
): Map<number, number> {
  const frequency = new Map<number, number>();

  for (const event of events) {
    const bucket = Math.floor(event.timestamp / periodMs) * periodMs;
    frequency.set(bucket, (frequency.get(bucket) || 0) + 1);
  }

  return frequency;
}

/**
 * Find events within time range
 */
export function getEventsInTimeRange<T extends { timestamp: number }>(
  events: T[],
  startTime: number,
  endTime: number
): T[] {
  return events.filter((e) => e.timestamp >= startTime && e.timestamp <= endTime);
}

/**
 * Get latest N events
 */
export function getLatestEvents<T extends { timestamp?: number }>(
  events: T[],
  count: number
): T[] {
  const sorted = sortEventsByTimestamp(events, false);
  return sorted.slice(0, count);
}

/**
 * Check if event matches pattern
 */
export function eventMatchesPattern(eventType: string, pattern: string): boolean {
  // Support wildcards: 0x2::coin::* or *::transfer::*
  const regexPattern = pattern
    .replace(/\*/g, '[^:]+')
    .replace(/::/g, '::');

  const regex = new RegExp(`^${regexPattern}$`);
  return regex.test(eventType);
}

/**
 * Extract field from event data
 */
export function extractEventField<T = any>(
  event: { parsedJson?: Record<string, any> },
  fieldPath: string
): T | undefined {
  if (!event.parsedJson) return undefined;

  const parts = fieldPath.split('.');
  let current: any = event.parsedJson;

  for (const part of parts) {
    if (current && typeof current === 'object' && part in current) {
      current = current[part];
    } else {
      return undefined;
    }
  }

  return current as T;
}
