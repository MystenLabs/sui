// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ArrowUpRight16 } from '@mysten/icons';
import { Text, Heading } from '@mysten/ui';
import { cva } from 'class-variance-authority';
import { useState } from 'react';

import { ObjectLink } from './InternalLink';
import { ObjectVideoImage } from '~/ui/ObjectVideoImage';

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
	noTypeRender?: boolean;
}

export function ObjectDetails({
	id,
	image = '',
	name = '',
	video,
	type,
	variant = 'small',
	noTypeRender,
}: ObjectDetailsProps) {
	const [open, setOpen] = useState(false);
	const openPreview = () => setOpen(true);

	return (
		<div className="flex items-center gap-3.75 overflow-auto">
			<ObjectVideoImage
				title={name}
				subtitle={type}
				src={image}
				video={video}
				variant={variant}
				open={open}
				setOpen={setOpen}
			/>
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
				{!noTypeRender && (
					<Text variant="bodySmall/medium" color="steel-dark" truncate>
						{type}
					</Text>
				)}
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
