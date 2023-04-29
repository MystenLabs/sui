// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCopyToClipboard } from '@mysten/core';
import { Copy12 } from '@mysten/icons';
import { toast } from 'react-hot-toast';

import { Link } from '~/ui/Link';

export interface CopyToClipboardProps {
    copyText: string;
    onSuccessMessage?: string;
}

export function CopyClipboard({
    copyText,
    onSuccessMessage = 'Copied!',
}: CopyToClipboardProps) {
    const copyToClipBoard = useCopyToClipboard(() =>
        toast.success(onSuccessMessage)
    );

    return (
        <Link onClick={() => copyToClipBoard(copyText)}>
            <Copy12 className="h-3 w-3 cursor-pointer text-gray-45 hover:text-steel" />
        </Link>
    );
}
