// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

type FileExtensionType = {
	[key: string]: {
		name: string;
		type: string;
	};
};

// Soft file type detection.
// Returns the file type of the given file name.
// Temporary solution until we have a better way to detect file types.
// extentionType: FileExtensionType
//    type?: 'image' | 'audio' | 'video';
const FILE_EXTENSION_TYPE_MAP: FileExtensionType = {
	jpeg: {
		name: 'JPEG',
		type: 'image',
	},
	jpg: {
		name: 'JPEG',
		type: 'image',
	},
	png: {
		name: 'PNG',
		type: 'image',
	},
	gif: {
		name: 'GIF',
		type: 'image',
	},
	bmp: {
		name: 'BMP',
		type: 'image',
	},
	webp: {
		name: 'WEBP',
		type: 'image',
	},
	svg: {
		name: 'SVG',
		type: 'image',
	},
};

/*
// TODO: extend this list with more file types.
const FILE_EXTENSION_TYPE_MAP_HEADERS: FileExtensionType = {
    'image/jpeg': {
        name: 'JPEG',
        type: 'image',
    },
    'image/png': {
        name: 'PNG',
        type: 'image',
    },
    'image/gif': {
        name: 'GIF',
        type: 'image',
    },
    'image/apng': {
        name: 'APNG',
        type: 'image',
    },
    'image/svg+xml': {
        name: 'SVG',
        type: 'image',
    },
    'image/webp': {
        name: 'WEBP',
        type: 'image',
    },
    'audio/wave': {
        name: 'Wave',
        type: 'audio',
    },
    'audio/wav': {
        name: 'WEBP',
        type: 'audio',
    },
    'audio/x-wav': {
        name: 'WEBP',
        type: 'audio',
    },
    'audio/x-pn-wav': {
        name: 'WEBP',
        type: 'audio',
    },
};

export const extractFileType = async (imgUrl: string) => {
    return fetch(imgUrl)
        .then((resp) => resp?.headers?.get('Content-Type') || false)
        .catch((err) => {
            return false;
        });
};
 */
export default function useFileExtensionType(url: string) {
	if (!url) return { name: '', type: '' };
	const fileType = url.split('.').pop() || '';
	return FILE_EXTENSION_TYPE_MAP[fileType] || { name: '', type: '' };
}
