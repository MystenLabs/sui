// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCopyToClipboard } from '@mysten/core';
import { Copy12, Copy16, CopyNew24 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import { toast } from 'react-hot-toast';

import { Link } from '~/ui/Link';

const iconStyles = cva(['cursor-pointer hover:text-steel-dark'], {
    variants: {
        size: {
            sm: 'w-3 h-3',
            md: 'w-4 h-4',
            lg: 'w-5 h-5',
        },
        color: {
            gray45: 'text-gray-45',
            steel: 'text-steel',
        },
    },
    defaultVariants: {
        size: 'md',
        color: 'gray45',
    },
});

export type IconStylesProps = VariantProps<typeof iconStyles>;

export interface CopyToClipboardProps extends IconStylesProps {
    copyText: string;
    onSuccessMessage?: string;
}

const ICON_SIZES = {
    sm: Copy12,
    md: Copy16,
    lg: CopyNew24,
};

export function CopyToClipboard({
    copyText,
    color,
    size = 'md',
    onSuccessMessage = 'Copied!',
}: CopyToClipboardProps) {
    const copyToClipBoard = useCopyToClipboard(() =>
        toast.success(onSuccessMessage)
    );

    const CopyIcon = ICON_SIZES[size!];

    return (
        <Link onClick={() => copyToClipBoard(copyText)}>
            <span className="sr-only">Copy</span>
            <CopyIcon className={iconStyles({ size, color })} />
        </Link>
    );
}
