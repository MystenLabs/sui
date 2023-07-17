// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ImportValuesType } from '_pages/initialize/import/index';

export type StepProps = {
	next: (data: ImportValuesType, step: 1 | -1) => Promise<void>;
	data: ImportValuesType;
	mode: 'import' | 'forgot';
};
