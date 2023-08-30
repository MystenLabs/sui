// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin } from '@mysten/core';
import { ArrowUpRight16 } from '@mysten/icons';
import { normalizeSuiAddress } from '@mysten/sui.js';
import { type DisplayFieldsResponse, type SuiObjectResponse } from '@mysten/sui.js/client';
import { parseStructTag, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { Heading, Text } from '@mysten/ui';
import { type ReactNode, useEffect, useState } from 'react';

import { useResolveVideo } from '~/hooks/useResolveVideo';
import { LinkOrTextDescriptionItem } from '~/pages/object-result/LinkOrTextDescriptionItem';
import { Card } from '~/ui/Card';
import { Divider } from '~/ui/Divider';
import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { ObjectVideoImage } from '~/ui/ObjectVideoImage';
import { extractName, getDisplayUrl, parseImageURL, parseObjectType } from '~/utils/objectUtils';
import { genFileTypeMsg, trimStdLibPrefix } from '~/utils/stringUtils';

interface HeroVideoImageProps {
	title: string;
	subtitle: string;
	src: string;
	variant: 'xl';
	video?: string | null;
}

function HeroVideoImage({ title, subtitle, src, video, variant }: HeroVideoImageProps) {
	const [open, setOpen] = useState(false);

	return (
		<div className="group relative">
			<ObjectVideoImage
				title={title}
				subtitle={subtitle}
				src={src}
				video={video}
				variant={variant}
				open={open}
				setOpen={setOpen}
				rounded="xl"
			/>
			<ArrowUpRight16 className="right-5 top-5 hidden group-hover:absolute group-hover:block" />
		</div>
	);
}

function ObjectViewItem({ title, children }: { title: string; children: ReactNode }) {
	return (
		<div className="flex justify-between gap-40">
			<Text variant="pBodySmall/medium" color="steel-dark">
				{title}
			</Text>
			{children}
		</div>
	);
}

function ObjectViewCard({ children }: { children: ReactNode }) {
	return (
		<div className="mb-6 h-full min-w-[50%] basis-1/2 pr-6">
			<Card bg="white/80" spacing="lg" height="full">
				<div className="flex flex-col gap-4">{children}</div>
			</Card>
		</div>
	);
}

function LinkWebsite({ value }: { value: string }) {
	const urlData = getDisplayUrl(value);

	if (!urlData) {
		return null;
	}

	if (typeof urlData === 'string') {
		return <Text variant="pBodySmall/medium">{urlData}</Text>;
	}

	return (
		<Link href={urlData.href} variant="textHeroDark">
			{urlData.display}
		</Link>
	);
}

interface ObjectViewProps {
	data: SuiObjectResponse;
}
export function ObjectView({ data }: ObjectViewProps) {
	const [fileType, setFileType] = useState<undefined | string>(undefined);
	const display = data.data?.display?.data;
	const imgUrl = parseImageURL(display);
	const video = useResolveVideo(data);
	const name = extractName(display);
	const objectType = parseObjectType(data);
	const objOwner = data.data?.owner;
	const storageRebate = data.data?.storageRebate;

	const linkUrl = getDisplayUrl(display?.link);
	const projectUrl = getDisplayUrl(display?.project_url);

	const [storageRebateFormatted, symbol] = useFormatCoin(
		storageRebate,
		SUI_TYPE_ARG,
		CoinFormat.FULL,
	);

	const { address, module, name: parseStructTagName } = parseStructTag(objectType);

	const genhref = (objType: string) => {
		const metadataarr = objType.split('::');
		const address = normalizeSuiAddress(metadataarr[0]);
		return `/object/${address}?module=${metadataarr[1]}`;
	};

	useEffect(() => {
		const controller = new AbortController();
		genFileTypeMsg(imgUrl, controller.signal)
			.then((result) => setFileType(result))
			.catch((err) => console.log(err));

		return () => {
			controller.abort();
		};
	}, [imgUrl]);

	return (
		<div className="flex gap-6">
			{imgUrl !== '' && (
				<HeroVideoImage
					title={name || display?.description || trimStdLibPrefix(objectType)}
					subtitle={video ? 'Video' : fileType ?? ''}
					src={imgUrl}
					video={video}
					variant="xl"
				/>
			)}

			<div className="flex h-full w-full flex-row flex-wrap">
				<ObjectViewCard>
					<Heading variant="heading4/semibold" color="steel-darker">
						{name || display?.description}
					</Heading>
					{name && display && (
						<Text variant="pBody/normal" color="steel-darker">
							{display.description}
						</Text>
					)}

					<Divider />

					<ObjectViewItem title="Object ID">
						<ObjectLink objectId={data.data?.objectId!} />
					</ObjectViewItem>

					<ObjectViewItem title="Type">
						<ObjectLink
							label={<div className="text-right">{trimStdLibPrefix(objectType)}</div>}
							objectId={`${address}?module=${module}`}
						/>
					</ObjectViewItem>
				</ObjectViewCard>

				{objOwner && (
					<ObjectViewCard>
						<ObjectViewItem title="Owner">
							{objOwner === 'Immutable' ? (
								'Immutable'
							) : 'Shared' in objOwner ? (
								'Shared'
							) : 'ObjectOwner' in objOwner ? (
								<ObjectLink objectId={objOwner.ObjectOwner} />
							) : (
								<AddressLink address={objOwner.AddressOwner} />
							)}
						</ObjectViewItem>
					</ObjectViewCard>
				)}

				<ObjectViewCard>
					<ObjectViewItem title="Version">
						<Text variant="pBodySmall/medium" color="gray-90">
							{data.data?.version}
						</Text>
					</ObjectViewItem>

					<ObjectViewItem title="Last Transaction Block Digest">
						<TransactionLink digest={data.data?.previousTransaction!} />
					</ObjectViewItem>
				</ObjectViewCard>

				{display && (
					<ObjectViewCard>
						<ObjectViewItem title="Link">
							<LinkWebsite value={display.link} />
						</ObjectViewItem>

						<ObjectViewItem title="Website">
							<LinkWebsite value={display.project_url} />
						</ObjectViewItem>
					</ObjectViewCard>
				)}

				<ObjectViewCard>
					<ObjectViewItem title="Storage Rebate">
						<Text variant="pBodySmall/medium" color="steel-darker">
							-{storageRebateFormatted}
						</Text>
					</ObjectViewItem>
				</ObjectViewCard>
			</div>
		</div>
	);
}
