// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useIntl } from 'react-intl';

export default function useNumberDelimiters() {
    const intl = useIntl();
    const parts = useMemo(() => intl.formatNumberToParts(12345.6), [intl]);
    const groupDelimiter = useMemo(
        () => parts.find((aPart) => aPart.type === 'group')?.value || null,
        [parts]
    );
    const decimalDelimiter = useMemo(
        () => parts.find((aPart) => aPart.type === 'decimal')?.value || null,
        [parts]
    );
    return useMemo(
        () => ({ groupDelimiter, decimalDelimiter }),
        [groupDelimiter, decimalDelimiter]
    );
}
