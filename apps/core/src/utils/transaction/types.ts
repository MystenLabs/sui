// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiAddress } from '@mysten/sui.js';

import { BalanceChangeSummary } from './getBalanceChangeSummary';
import { GasSummaryType } from './getGasSummary';
import { ObjectChangeSummary } from './getObjectChangeSummary';

export type TransactionSummary = {
	digest?: string;
	sender?: SuiAddress;
	timestamp?: string;
	balanceChanges: BalanceChangeSummary;
	gas?: GasSummaryType;
	objectSummary: ObjectChangeSummary | null;
} | null;

export type SuiObjectChangeTypes =
	| 'published'
	| 'transferred'
	| 'mutated'
	| 'deleted'
	| 'wrapped'
	| 'created';
