// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

export default function useMiddleEllipsis(txt: string, maxLength = 14) {
    return useMemo(() => {
        if (txt.length < maxLength + 3) {
            return txt;
        }
        const beginningLength = Math.ceil(maxLength / 2);
        const endingLength = maxLength - beginningLength;
        return `${txt.substring(0, beginningLength)}...${txt.substring(
            txt.length - endingLength
        )}`;
    }, [txt, maxLength]);
}
