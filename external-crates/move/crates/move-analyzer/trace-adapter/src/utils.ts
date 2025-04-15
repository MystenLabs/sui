// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Describes a Move module.
 */
export interface ModuleInfo {
    addr: string;
    name: string;
}

/**
 * If end of lifetime for a local has this value,
 * it means that it lives until the end of the current
 * frame.
 */
export const FRAME_LIFETIME = -1;

/**
 * The extension for JSON files.
 */
export const JSON_FILE_EXT = ".json";
