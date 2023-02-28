// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { EyeClose16, NftTypeImage24 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import { Heading } from './Heading';
import { ObjectLink } from './InternalLink';
import { LightBox } from './LightBox';
import { Text } from './Text';

interface ImageProps {
    onClick?: () => void;
    className: string;
    src: string;
    blur?: boolean;
    alt?: string;
}

const styles = cva(['object-cover'], {
    variants: {
        size: {
            small: 'h-14 w-14',
            large: 'h-32 w-32',
        },
    },
});

type ObjectDetailsStylesProps = VariantProps<typeof styles>;

export interface ObjectDetailsProps
    extends Omit<ObjectDetailsStylesProps, 'size'> {
    id?: string;
    image: string;
    name?: string;
    type: string;
    nsfw?: boolean;
    variant: 'small' | 'large';
}

export function ObjectDetails({
    id,
    image,
    name,
    type,
    nsfw = false,
    variant = 'small',
    ...styleProps
}: ObjectDetailsProps) {
    const [open, setOpen] = useState(false);
    const close = () => setOpen(false);
    const openPreview = () => setOpen(true);
    return (
        <div className="flex items-center gap-3.75">
            <ObjectPreview open={open} onClose={close}>
                <div className="flex flex-col gap-5">
                    <Image alt={name} src={image} className="rounded-none" />
                    <Heading variant="heading2/semibold" color="sui-light">
                        {name}
                    </Heading>
                    <Text color="gray-60" variant="body/medium">
                        {type}
                    </Text>
                </div>
            </ObjectPreview>
            <div className="relative">
                <Image
                    onClick={openPreview}
                    alt={name}
                    src={image}
                    blur={nsfw}
                    className={styles({ size: variant })}
                />
            </div>

            <div className="flex min-w-0 flex-col flex-nowrap gap-1.25">
                <Text variant="bodySmall/medium" color="gray-90">
                    {name}
                </Text>
                {id && <ObjectLink objectId={id} />}
                <Text variant="bodySmall/medium" color="steel-dark">
                    {type}
                </Text>
            </div>
        </div>
    );
}

function FallbackImage({ className }: { className: string }) {
    return (
        <div
            className={clsx(
                className,
                'flex items-center justify-center rounded-md bg-gray-45 text-gray-65'
            )}
        >
            <NftTypeImage24 />
        </div>
    );
}

function Image({ className, alt, src, blur = false, ...rest }: ImageProps) {
    const [error, setError] = useState(false);
    if (error) return <FallbackImage className={className} />;
    return (
        <>
            {blur ? (
                <div className="pointer-events-none absolute z-20 flex h-full w-full flex-col items-center justify-center space-y-2.5 rounded-md bg-gray-100/30 text-center backdrop-blur-sm">
                    <EyeClose16 className="h-1/3 w-1/3 text-white" />
                </div>
            ) : null}
            {error ? (
                <FallbackImage className={className} />
            ) : (
                <img
                    onError={() => setError(true)}
                    alt={alt}
                    src={src ?? ''}
                    className={clsx('z-0 cursor-pointer rounded-md', className)}
                    {...rest}
                />
            )}
        </>
    );
}

interface ObjectPreviewProps {
    children: ReactNode;
    open: boolean;
    onClose: () => void;
}

function ObjectPreview({ children, open, onClose }: ObjectPreviewProps) {
    return (
        <LightBox open={open} onClose={onClose}>
            {children}
        </LightBox>
    );
}
