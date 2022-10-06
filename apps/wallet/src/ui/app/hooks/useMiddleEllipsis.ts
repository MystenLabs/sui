// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

export default function useMiddleEllipsis(
    txt: string | null,
    maxLength = 10,
    maxLengthBeginning = 6
) {
    return useMemo(() => {
        if (!txt) {
            return '';
        }
        if (txt.length < maxLength + 3) {
            return txt;
        }
        let beginningLength = maxLengthBeginning || Math.ceil(maxLength / 2);
        if (beginningLength >= maxLength) {
            beginningLength = Math.ceil(maxLength / 2);
            // eslint-disable-next-line no-console
            console.warn(
                `[useMiddleEllipsis]: maxLengthBeginning (${maxLengthBeginning}) is equal or bigger than maxLength (${maxLength})`
            );
        }
        const endingLength = maxLength - beginningLength;
        return `${txt.substring(0, beginningLength)}...${txt.substring(
            txt.length - endingLength
        )}`;
    }, [txt, maxLength, maxLengthBeginning]);
}
