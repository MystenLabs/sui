// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentType } from 'react';

/**
 * A helper that can extract props from a React component type.
 * Normally, you can use React.ComponentProps for this, but for some more complex
 * React type definitions, that helper does not work.
 */
export type ExtractProps<T> = T extends ComponentType<infer P> ? P : T;
