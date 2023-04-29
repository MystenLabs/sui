// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

export function useCopyToClipboard(callbackOnSuccess?: () => void) {
    return useCallback(
        async (text: string) => {
            if (!navigator?.clipboard) {
                console.warn('Clipboard not supported');
                return false;
            }

            try {
                await navigator.clipboard.writeText(text);
                if (callbackOnSuccess) {
                    callbackOnSuccess();
                }
                return true;
            } catch (error) {
                console.warn('Copy failed', error);
                return false;
            }
        },
        [callbackOnSuccess]
    );
}
