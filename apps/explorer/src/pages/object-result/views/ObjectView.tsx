// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin, useResolveSuiNSName } from '@mysten/core';
import { ArrowUpRight16, Info16 } from '@mysten/icons';
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
import { Description } from '~/ui/Description';
import { Divider } from '~/ui/Divider';
import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { ObjectVideoImage } from '~/ui/ObjectVideoImage';
import { extractName, getDisplayUrl, parseImageURL, parseObjectType } from '~/utils/objectUtils';
import { genFileTypeMsg, trimStdLibPrefix } from '~/utils/stringUtils';
import { Tooltip } from '~/ui/Tooltip';

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
			<div className="absolute right-3 top-3 hidden h-8 w-8 items-center justify-center rounded-md bg-white/40 backdrop-blur group-hover:flex">
				<ArrowUpRight16 />
			</div>
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
	const { address, module, typeParams, ...rest } = parseStructTag(objectType);

	const formattedTypeParams = typeParams.map((typeParam) => {
		if (typeof typeParam === 'string') {
			return typeParam;
		} else {
			return {
				...typeParam,
				address: formatAddress(typeParam.address),
			};
		}
	});

	const structTag = {
		address: formatAddress(address),
		module,
		typeParams: formattedTypeParams,
		...rest,
	};

	const normalizedStructTag = normalizeStructTag(structTag);
	const objectNameDisplay = name || display?.description;
	const renderDescription = name && display?.description;

	return (
		<ObjectViewCard>
			{objectNameDisplay && (
				<Heading variant="heading4/semibold" color="steel-darker">
					{objectNameDisplay}
				</Heading>
			)}
			{renderDescription && (
				<Text variant="pBody/normal" color="steel-darker">
					{display.description}
				</Text>
			)}

			<Description title="Object ID">
				<ObjectLink objectId={objectId} />
			</Description>

			<Description
				title={
					<div className="flex items-center gap-1">
						<Text variant="pBodySmall/medium" color="steel-dark">
							Type
						</Text>

						<Tooltip tip={<div className="flex flex-wrap break-all">{objectType}</div>}>
							<Info16 />
						</Tooltip>
					</div>
				}
			>
				<ObjectLink
					label={<div className="text-right">{normalizedStructTag}</div>}
					objectId={`${address}?module=${module}`}
				/>
			</Description>
		</ObjectViewCard>
	);
}

function VersionCard({ version, digest }: { version?: string; digest: string }) {
	return (
		<ObjectViewCard>
			<Description title="Version">
				<Text variant="pBodySmall/medium" color="gray-90">
					{version}
				</Text>
			</Description>

			<Description title="Last Transaction Block Digest">
				<TransactionLink digest={digest} />
			</Description>
		</ObjectViewCard>
	);
}

function AddressOwner({ address }: { address: string }) {
	const { data: suinsDomainName } = useResolveSuiNSName(address);

	return <AddressLink address={address} label={suinsDomainName} />;
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
				<Description title="Owner">
					{objOwner === 'Immutable' ? (
						<Text variant="pBodySmall/medium" color="gray-90">
							Immutable
						</Text>
					) : 'Shared' in objOwner ? (
						<Text variant="pBodySmall/medium" color="gray-90">
							Shared
						</Text>
					) : 'ObjectOwner' in objOwner ? (
						<ObjectLink objectId={objOwner.ObjectOwner} />
					) : (
						<AddressOwner address={objOwner.AddressOwner} />
					)}
				</Description>
			)}

			{display && (display.link || display.project_url) && (
				<>
					<Divider />

					{display.link && (
						<Description title="Link">
							<LinkWebsite value={display.link} />
						</Description>
					)}

					{display.project_url && (
						<Description title="Website">
							<LinkWebsite value={display.project_url} />
						</Description>
					)}
				</>
			)}

			<Divider />

			<Description title="Storage Rebate">
				<Text variant="pBodySmall/medium" color="steel-darker">
					-{storageRebateFormatted} SUI
				</Text>
			</Description>
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
	const objectId = data.data?.objectId;
	const lastTransactionBlockDigest = data.data?.previousTransaction;

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

			{objectId && (
				<div style={{ gridArea: 'description' }}>
					<DescriptionCard
						name={name}
						objectType={objectType}
						objectId={objectId}
						display={display}
					/>
				</div>
			)}

			{lastTransactionBlockDigest && (
				<div style={{ gridArea: 'version' }}>
					<VersionCard version={data.data?.version} digest={lastTransactionBlockDigest} />
				</div>
			)}

			<div style={{ gridArea: 'owner' }}>
				<OwnerCard objOwner={objOwner} display={display} storageRebate={storageRebate} />
			</div>
		</div>
	);
}
