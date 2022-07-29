// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

// Soft file type detection.
// Returns the file type of the given file name.
const FILE_EXTENSION_TYPE_MAP = {
    'jpeg': 'jpeg',
    'jpg': 'jpeg',
    'png': 'png',
    'gif': 'gif',
    'bmp': 'bmp',
    'webp': 'webp',
    'svg': 'svg',

}
export default function useFileExtentionType(url: string) {
    if (!url) return false;
    const fileType =  url.split('.').pop();
    if(!fileType) return 'Unknown';
    return useMemo(() => FILE_EXTENSION_TYPE_MAP[fileType], [fileType]);
    return useMemo(
        () => ({ groupDelimiter, decimalDelimiter }),
        [groupDelimiter, decimalDelimiter]
    );
}
