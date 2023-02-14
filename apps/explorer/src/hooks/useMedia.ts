// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useImageMod } from './useImageMod';

function useMedia(url: string) {
    const formatted = url.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');
    const { data: allowed } = useImageMod({ url: formatted });
    return { url: formatted, nsfw: !allowed };
}

export default useMedia;
