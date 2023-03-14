// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ArrowUpRight16, X12 } from '@mysten/icons';
import { cva } from 'class-variance-authority';
import { useState } from 'react';
import useImage from '~/hooks/useImage';
import { useImageMod } from '~/hooks/useImageMod';

import { Heading } from './Heading';
import { IconButton } from './IconButton';
import { Image } from './image/Image';
import { ObjectLink } from './InternalLink';
import { Modal } from './Modal';
import { Text } from './Text';

const imageStyles = cva(['rounded-md cursor-pointer object-cover z-0'], {
    variants: {
        size: {
            small: 'h-14 w-14',
            large: 'h-32 w-32',
        },
    },
});

const textStyles = cva(['flex min-w-0 flex-col flex-nowrap'], {
    variants: {
        size: {
            small: 'gap-1.25',
            large: 'gap-2.5',
        },
    },
});

export interface ObjectDetailsProps {
    id?: string;
    image: string;
    name?: string;
    type: string;
    nsfw: boolean | null;
    variant: 'small' | 'large';
}

export function ObjectDetails({
    id,
    image,
    name,
    type,
    nsfw,
    variant = 'small',
}: ObjectDetailsProps) {
    const [open, setOpen] = useState(false);
    const close = () => setOpen(false);
    const openPreview = () => setOpen(true);

    return (
        <div className="flex items-center gap-3.75">
            <Modal open={open} onClose={close}>
                <div className="flex flex-col gap-5">
                    <Image alt={name} src={image} rounded="none" />
                    <Heading variant="heading2/semibold" color="sui-light">
                        {name}
                    </Heading>
                    <Text color="gray-60" variant="body/medium">
                        {type}
                    </Text>
                </div>
                <div className="absolute -right-12 top-0">
                    <IconButton
                        onClick={close}
                        className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-full border-0 bg-gray-90 p-0 text-sui-light outline-none hover:scale-105 active:scale-100"
                        aria-label="Close"
                        icon={X12}
                    />
                </div>
            </Modal>

            <div className={imageStyles({ size: variant })}>
                <Image
                    rounded="md"
                    onClick={openPreview}
                    alt={name}
                    src={image}
                />
            </div>
            <div className={textStyles({ size: variant })}>
                {variant === 'large' ? (
                    <Heading color="gray-90" variant="heading4/semibold">
                        {name}
                    </Heading>
                ) : (
                    <Text variant="bodySmall/medium" color="gray-90">
                        {name}
                    </Text>
                )}
                {id && <ObjectLink objectId={id} />}
                <Text variant="bodySmall/medium" color="steel-dark" truncate>
                    {type}
                </Text>
                {variant === 'large' ? (
                    <div
                        onClick={openPreview}
                        className="mt-2.5 flex cursor-pointer items-center gap-1 text-steel-dark"
                    >
                        <Text variant="caption/semibold">Preview</Text>
                        <ArrowUpRight16 className="inline-block" />
                    </div>
                ) : null}
            </div>
        </div>
    );
}
