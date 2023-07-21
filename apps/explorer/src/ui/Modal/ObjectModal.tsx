// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X12 } from '@mysten/icons';
import { Text, Heading, IconButton } from '@mysten/ui';

import { Modal, type ModalProps } from './index';
import { Image } from '../image/Image';

export interface ObjectModalProps extends Omit<ModalProps, 'children'> {
	title: string;
	subtitle: string;
	alt: string;
	src: string;
	video?: string | null;
	moderate?: boolean;
}

export function ObjectModal({
	open,
	onClose,
	alt,
	title,
	subtitle,
	src,
	video,
	// NOTE: Leave false only if ObjectModal is paired with an Image component
	moderate = false,
}: ObjectModalProps) {
	return (
		<Modal open={open} onClose={onClose}>
			<div className="flex flex-col gap-5">
				{video ? (
					<video controls className="h-full w-full" src={video} />
				) : (
					// Moderation is disabled inside the modal so if a user clicks to open an unblurred image the experience is consistent
					<Image alt={alt} src={src} rounded="none" moderate={moderate} />
				)}
				<div className="flex flex-col gap-3">
					<Heading variant="heading2/semibold" color="sui-light" truncate>
						{title}
					</Heading>
					<Text color="gray-60" variant="body/medium">
						{subtitle}
					</Text>
				</div>
			</div>
			<div className="absolute -right-12 top-0">
				<IconButton
					onClick={onClose}
					className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-full border-0 bg-gray-90 p-0 text-sui-light outline-none hover:scale-105 active:scale-100"
					aria-label="Close"
				>
					<X12 />
				</IconButton>
			</div>
		</Modal>
	);
}
