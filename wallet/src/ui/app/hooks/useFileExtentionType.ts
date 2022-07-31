// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

type FileExtentionType = {
    [key: string]: string;
};

// Soft file type detection.
// Returns the file type of the given file name.
// Temporary solution until we have a better way to detect file types.
// extentionType: FileExtentionType
//    type?: 'image' | 'audio' | 'video';
const FILE_EXTENSION_TYPE_MAP: FileExtentionType = {
    jpeg: 'JPEG Image',
    jpg: 'JPEG Image',
    png: 'PNG Image',
    gif: 'GIF Image',
    bmp: 'BMP Image',
    webp: 'WEBP Image',
    svg: 'SVG Image',
};
export default function useFileExtentionType(url: string) {
    if (!url) return false;
    const fileType = url.split('.').pop();
    if (!fileType) return 'Unknown';

    return FILE_EXTENSION_TYPE_MAP[fileType] || 'Unknown';
}
