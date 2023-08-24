// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { Placeholder, Text } from '@mysten/ui';
import { type ReactNode, useState } from 'react';

import { useResolveVideo } from '~/hooks/useResolveVideo';
import { ObjectLink } from '~/ui/InternalLink';
import { ObjectVideoImage } from '~/ui/ObjectVideoImage';
import { parseObjectType } from '~/utils/objectUtils';
import { trimStdLibPrefix } from '~/utils/stringUtils';

function ListViewItem({
	asset,
	type,
	objectId,
	loading,
	onClick,
}: {
	asset?: ReactNode;
	type?: ReactNode;
	objectId?: ReactNode;
	loading?: boolean;
	onClick?: () => void;
}) {
	return (
		<div
			onClick={onClick}
			className="group mb-2 flex items-center justify-between rounded-lg p-1 hover:bg-hero/5"
		>
			<div className="flex max-w-[66%] basis-8/12 items-center gap-3 md:max-w-[25%] md:basis-3/12 md:pr-5">
				{loading ? <Placeholder rounded="lg" width="540px" height="20px" /> : asset}
			</div>

			<div className="hidden max-w-[50%] basis-6/12 pr-5 md:flex">
				{loading ? <Placeholder rounded="lg" width="540px" height="20px" /> : type}
			</div>

			<div className="flex max-w-[34%] basis-3/12">
				{loading ? <Placeholder rounded="lg" width="540px" height="20px" /> : objectId}
			</div>
		</div>
	);
}

function ListViewItemContainer({ obj }: { obj: SuiObjectResponse }) {
	const [open, setOpen] = useState(false);
	const video = useResolveVideo(obj);
	const displayMeta = obj.data?.display?.data;
	const name = displayMeta?.name ?? displayMeta?.description ?? '';
	const type = trimStdLibPrefix(parseObjectType(obj));
	const objectId = obj.data?.objectId;

	return (
		<ListViewItem
			asset={
				<>
					<ObjectVideoImage
						title={name}
						subtitle={type}
						src={displayMeta?.image_url || ''}
						video={video}
						variant="xs"
						open={open}
						setOpen={setOpen}
					/>
					<div className="flex flex-col overflow-hidden">
						<Text variant="subtitle/semibold" color="steel-darker" truncate>
							{name || '--'}
						</Text>
						<div className="block md:hidden">
							<Text variant="pSubtitle/normal" color="steel-dark" truncate>
								{type}
							</Text>
						</div>
					</div>
				</>
			}
			type={
				<Text variant="bodySmall/medium" color="steel-dark" truncate>
					{type}
				</Text>
			}
			objectId={
				<Text variant="bodySmall/medium" color="steel-dark" truncate>
					{objectId && <ObjectLink objectId={objectId} />}
				</Text>
			}
		/>
	);
}

interface ListViewProps {
	data?: SuiObjectResponse[];
	loading?: boolean;
}

export function ListView({ data, loading }: ListViewProps) {
	return (
		<div className="flex flex-col overflow-auto">
			<div className="mb-3.5 flex w-full justify-around">
				<div className="max-w-[66%] basis-8/12 md:max-w-[25%] md:basis-3/12">
					<Text variant="caption/medium" color="steel-dark">
						ASSET
					</Text>
				</div>
				<div className="hidden basis-6/12 md:block">
					<Text variant="caption/medium" color="steel-dark">
						TYPE
					</Text>
				</div>
				<div className="basis-3/12">
					<Text variant="caption/medium" color="steel-dark">
						OBJECT ID
					</Text>
				</div>
			</div>
			{loading && new Array(10).fill(0).map((_, index) => <ListViewItem key={index} loading />)}

			<div className="flex h-full w-full flex-col overflow-auto">
				{data?.map((obj) => {
					if (!obj.data) {
						return null;
					}
					return <ListViewItemContainer key={obj.data.objectId} obj={obj} />;
				})}
			</div>
		</div>
	);
}
