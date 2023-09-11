// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type PartialBy<T, K extends keyof T> = Omit<T, K> & Partial<T>;
