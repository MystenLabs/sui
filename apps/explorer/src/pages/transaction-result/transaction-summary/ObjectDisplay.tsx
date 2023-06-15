// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DisplayFieldsResponse } from '@mysten/sui.js';
import { useState } from 'react';

import { ObjectLink } from '~/ui/InternalLink';
import { ObjectModal } from '~/ui/Modal/ObjectModal';
import { Image } from '~/ui/image/Image';

export function ObjectDisplay({
	objectId,
	display,
}: {
	objectId: string;
	display: DisplayFieldsResponse;
}) {
	const [open, handleOpen] = useState(false);
	if (!display.data) return null;
	const { description, name, image_url: imageUrl } = display.data ?? {};
	return (
		<div className="group relative w-32">
			<ObjectModal
				open={open}
				onClose={() => handleOpen(false)}
				title={name ?? description ?? ''}
				subtitle={description ?? ''}
				src={imageUrl ?? ''}
				alt={description ?? ''}
			/>
			<div className="relative w-32 cursor-pointer whitespace-nowrap">
				<Image
					size="lg"
					rounded="2xl"
					src={imageUrl ?? ''}
					alt={description}
					onClick={() => handleOpen(true)}
				/>
				<div className="absolute bottom-2 left-1/2 flex -translate-x-1/2 justify-center rounded-lg bg-white px-2 py-1 opacity-0 transition-opacity group-hover:opacity-100">
					<ObjectLink objectId={objectId} />
				</div>
			</div>
		</div>
	);
}
