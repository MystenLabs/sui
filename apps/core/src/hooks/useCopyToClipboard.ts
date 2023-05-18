// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

export function useCopyToClipboard(onSuccessCallback?: () => void) {
    return useCallback(
        async (text: string) => {
            if (!navigator?.clipboard) {
                return false;
            }

            try {
                await navigator.clipboard.writeText(text);
                if (onSuccessCallback) {
                    onSuccessCallback();
                }
                return true;
            } catch (error) {
                return false;
            }
        },
        [onSuccessCallback]
    );
}
