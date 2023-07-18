// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ArrowUpRight16, MediaPlay16 } from '@mysten/icons';
import { Text, Heading } from '@mysten/ui';
import { cva } from 'class-variance-authority';
import clsx from 'clsx';
import { useState } from 'react';

import { ObjectLink } from './InternalLink';
import { ObjectModal } from './Modal/ObjectModal';
import { Image } from './image/Image';

const imageStyles = cva(['cursor-pointer z-0 flex-shrink-0 relative'], {
	variants: {
		size: {
			small: 'h-16 w-16',
			large: 'h-50 w-50',
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
	video?: string | null;
	name?: string;
	type: string;
	variant: 'small' | 'large';
}

export function ObjectDetails({
	id,
	image = '',
	name = '',
	video,
	type,
	variant = 'small',
}: ObjectDetailsProps) {
	const [open, setOpen] = useState(false);
	const close = () => setOpen(false);
	const openPreview = () => setOpen(true);

	return (
		<div className="flex items-center gap-3.75 overflow-auto">
			<ObjectModal
				open={open}
				onClose={close}
				title={name}
				subtitle={type}
				src={image}
				video={video}
				alt={name}
			/>
			<div className={imageStyles({ size: variant })}>
				<Image rounded="md" onClick={openPreview} alt={name} src={image} />
				{video && (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80">
						<MediaPlay16
							className={clsx({
								'h-8 w-8': variant === 'large',
								'h-5 w-5': variant === 'small',
							})}
						/>
					</div>
				)}
			</div>
			<div className={textStyles({ size: variant })}>
				{variant === 'large' ? (
					<Heading variant="heading4/semibold" truncate>
						{name}
					</Heading>
				) : (
					<Text variant="bodySmall/medium" color="gray-90" truncate>
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
