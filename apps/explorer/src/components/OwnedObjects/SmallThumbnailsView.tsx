// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { formatAddress } from '@mysten/sui.js/utils';
import { Placeholder } from '@mysten/ui';
import { type ReactNode } from 'react';

import { OwnedObjectsText } from '~/components/OwnedObjects/OwnedObjectsText';
import { useResolveVideo } from '~/hooks/useResolveVideo';
import { ObjectLink } from '~/ui/InternalLink';
import { ObjectVideoImage } from '~/ui/ObjectVideoImage';
import { parseObjectType } from '~/utils/objectUtils';
import { trimStdLibPrefix } from '~/utils/stringUtils';

interface Props {
	limit: number;
	data?: SuiObjectResponse[];
	loading?: boolean;
}

function OwnObjectContainer({ id, children }: { id: string; children: ReactNode }) {
	return (
		<div className="w-full min-w-smallThumbNailsViewContainerMobile basis-1/2 pb-3 pr-4 md:min-w-smallThumbNailsViewContainer md:basis-1/4">
			<div className="rounded-lg p-2 hover:bg-hero/5">
				<ObjectLink display="block" objectId={id} label={children} />
			</div>
		</div>
	);
}

function SmallThumbnailsViewLoading({ limit }: { limit: number }) {
	return (
		<>
			{new Array(limit).fill(0).map((_, index) => (
				<OwnObjectContainer key={index} id={String(index)}>
					<Placeholder rounded="lg" height="80px" />
				</OwnObjectContainer>
			))}
		</>
	);
}

function SmallThumbnail({ obj }: { obj: SuiObjectResponse }) {
	const video = useResolveVideo(obj);
	const displayMeta = obj.data?.display?.data;
	const src = displayMeta?.image_url || '';
	const name = displayMeta?.name ?? displayMeta?.description ?? '--';
	const type = trimStdLibPrefix(parseObjectType(obj));
	const id = obj.data?.objectId;

	return (
		<div className="group flex items-center gap-3.75 overflow-auto">
			<ObjectVideoImage
				fadeIn
				disablePreview
				title={name}
				subtitle={type}
				src={src}
				video={video}
				variant="small"
			/>

			<div className="flex min-w-0 flex-col flex-nowrap gap-1.25">
				<OwnedObjectsText color="steel-darker" font="semibold">
					{name}
				</OwnedObjectsText>
				<OwnedObjectsText color="steel-dark" font="medium">
					{formatAddress(id!)}
				</OwnedObjectsText>
			</div>
		</div>
	);
}

export function SmallThumbnailsView({ data, loading, limit }: Props) {
	return (
		<div className="flex flex-row flex-wrap overflow-auto">
			{loading && <SmallThumbnailsViewLoading limit={limit} />}
			{data?.map((obj, index) => {
				const id = obj.data?.objectId;

				return (
					<OwnObjectContainer key={id} id={id!}>
						<SmallThumbnail obj={obj} />
					</OwnObjectContainer>
				);
			})}
		</div>
	);
}
