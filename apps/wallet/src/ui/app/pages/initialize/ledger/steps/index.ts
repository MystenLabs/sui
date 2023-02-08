// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { LedgerValuesType } from '_pages/initialize/ledger/index';

export type StepProps = {
    next: (data: LedgerValuesType, step: 1 | -1) => Promise<void>;
    data: LedgerValuesType;
    mode: 'import' | 'forgot';
};
