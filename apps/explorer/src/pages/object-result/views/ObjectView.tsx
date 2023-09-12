// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin } from '@mysten/core';
import { ArrowUpRight16 } from '@mysten/icons';
import { type ObjectOwner, type SuiObjectResponse } from '@mysten/sui.js/client';
import {
	formatAddress,
	normalizeStructTag,
	parseStructTag,
	SUI_TYPE_ARG,
} from '@mysten/sui.js/utils';
import { Heading, Text } from '@mysten/ui';
import { useQuery } from '@tanstack/react-query';
import clsx from 'clsx';
import { type ReactNode, useEffect, useState } from 'react';

import { useResolveVideo } from '~/hooks/useResolveVideo';
import { Card } from '~/ui/Card';
import { type DescriptionItemProps } from '~/ui/DescriptionList';
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
	video?: string | null;
}

function HeroVideoImage({ title, subtitle, src, video }: HeroVideoImageProps) {
	const [open, setOpen] = useState(false);

	return (
		<div className="group relative h-full">
			<ObjectVideoImage
				imgFit="contain"
				aspect="square"
				title={title}
				subtitle={subtitle}
				src={src}
				video={video}
				variant="fill"
				open={open}
				setOpen={setOpen}
				rounded="xl"
			/>
			<ArrowUpRight16 className="right-5 top-5 hidden group-hover:absolute group-hover:block" />
		</div>
	);
}

function ObjectViewItem({
	title,
	align,
	children,
}: {
	title: string;
	children: ReactNode;
	align?: DescriptionItemProps['align'];
}) {
	return (
		<div className={clsx('flex items-start justify-between gap-10')}>
			<Text variant="pBodySmall/medium" color="steel-dark">
				{title}
			</Text>

			{children}
		</div>
	);
}

function ObjectViewCard({ children }: { children: ReactNode }) {
	return (
		<Card bg="white/80" spacing="lg" height="full">
			<div className="flex flex-col gap-4">{children}</div>
		</Card>
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

function DescriptionCard({
	name,
	display,
	objectType,
	objectId,
}: {
	name?: string | null;
	display?: any;
	objectType: string;
	objectId: string;
}) {
	const { address, module, ...rest } = parseStructTag(objectType);

	const formattedAddress = formatAddress(address);
	const structTag = {
		address: formattedAddress,
		module,
		...rest,
	};

	const normalizedStructTag = normalizeStructTag(structTag);

	return (
		<ObjectViewCard>
			{name && (
				<Heading variant="heading4/semibold" color="steel-darker">
					{name}
				</Heading>
			)}
			{display?.description && (
				<Text variant="pBody/normal" color="steel-darker">
					{display.description}
				</Text>
			)}

			<ObjectViewItem title="Object ID">
				<ObjectLink objectId={objectId} />
			</ObjectViewItem>

			<ObjectViewItem title="Type" align="start">
				<ObjectLink
					label={<div className="text-right">{normalizedStructTag}</div>}
					objectId={`${address}?module=${module}`}
				/>
			</ObjectViewItem>
		</ObjectViewCard>
	);
}

function VersionCard({ version, digest }: { version?: string; digest: string }) {
	return (
		<ObjectViewCard>
			<ObjectViewItem title="Version">
				<Text variant="pBodySmall/medium" color="gray-90">
					{version}
				</Text>
			</ObjectViewItem>

			<ObjectViewItem title="Last Transaction Block Digest">
				<TransactionLink digest={digest} />
			</ObjectViewItem>
		</ObjectViewCard>
	);
}

function OwnerCard({
	objOwner,
	display,
	storageRebate,
}: {
	objOwner?: ObjectOwner | null;
	display?: {
		[key: string]: string;
	} | null;
	storageRebate?: string | null;
}) {
	const [storageRebateFormatted] = useFormatCoin(storageRebate, SUI_TYPE_ARG, CoinFormat.FULL);

	if (!objOwner && !display) {
		return null;
	}

	return (
		<ObjectViewCard>
			{objOwner && (
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
			)}

			{display && (display.link || display.project_url) && (
				<>
					<Divider />

					{display.link && (
						<ObjectViewItem title="Link">
							<LinkWebsite value={display.link} />
						</ObjectViewItem>
					)}

					{display.project_url && (
						<ObjectViewItem title="Website">
							<LinkWebsite value={display.project_url} />
						</ObjectViewItem>
					)}
				</>
			)}

			<Divider />

			<ObjectViewItem title="Storage Rebate">
				<Text variant="pBodySmall/medium" color="steel-darker">
					-{storageRebateFormatted} SUI
				</Text>
			</ObjectViewItem>
		</ObjectViewCard>
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

	const heroImageTitle = name || display?.description || trimStdLibPrefix(objectType);
	const heroImageSubtitle = video ? 'Video' : fileType ?? '';
	const heroImageProps = {
		title: heroImageTitle,
		subtitle: heroImageSubtitle,
		src: imgUrl,
		video: video,
	};

	const { data: imageData } = useQuery({
		queryKey: ['image-file-type', imgUrl],
		queryFn: ({ signal }) => genFileTypeMsg(imgUrl, signal!),
	});

	useEffect(() => {
		if (imageData) {
			setFileType(imageData);
		}
	}, [imageData]);

	return (
		<div className={clsx('address-grid-container-top', !imgUrl && 'no-image')}>
			{imgUrl !== '' && (
				<div style={{ gridArea: 'heroImage' }}>
					<HeroVideoImage {...heroImageProps} />
				</div>
			)}

			<div style={{ gridArea: 'description' }}>
				<DescriptionCard
					name={name}
					objectType={objectType}
					objectId={data.data?.objectId!}
					display={display}
				/>
			</div>

			<div style={{ gridArea: 'version' }}>
				<VersionCard version={data.data?.version} digest={data.data?.previousTransaction!} />
			</div>

			<div style={{ gridArea: 'owner' }}>
				<OwnerCard objOwner={objOwner} display={display} storageRebate={storageRebate} />
			</div>
		</div>
	);
}
