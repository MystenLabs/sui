// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ArrowUpRight16 } from '@mysten/icons';
import { cva } from 'class-variance-authority';
import { useState } from 'react';

import { Heading } from './Heading';
import { ObjectLink } from './InternalLink';
import { Text } from './Text';
import { Image } from './image/Image';
import { ImageModal } from './modal/ImageModal';

const imageStyles = cva(['cursor-pointer z-0 flex-shrink-0'], {
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
    image?: string;
    name?: string;
    type: string;
    variant: 'small' | 'large';
}

export function ObjectDetails({
    id,
    image = '',
    name = '',
    type,
    variant = 'small',
}: ObjectDetailsProps) {
    const [open, setOpen] = useState(false);
    const close = () => setOpen(false);
    const openPreview = () => setOpen(true);

    return (
        <div className="flex items-center gap-3.75">
            <ImageModal
                open={open}
                onClose={close}
                title={name}
                subtitle={type}
                src={image}
                alt={name}
            />
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
