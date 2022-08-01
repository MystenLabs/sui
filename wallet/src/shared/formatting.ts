// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { FormatNumberOptions } from 'react-intl';

export const balanceFormatOptions: FormatNumberOptions = {
    maximumFractionDigits: 0,
};

export const percentageFormatOptions: FormatNumberOptions = {
    style: 'percent',
    maximumFractionDigits: 2,
    minimumFractionDigits: 0,
};
