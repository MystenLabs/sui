// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin } from '@mysten/core';
import { ArrowUpRight16 } from '@mysten/icons';
import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { parseStructTag, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { Heading, Text } from '@mysten/ui';
import { type ReactNode, useEffect, useState } from 'react';

import { useResolveVideo } from '~/hooks/useResolveVideo';
import { Card } from '~/ui/Card';
import { DescriptionItem, type DescriptionItemProps } from '~/ui/DescriptionList';
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
		<DescriptionItem
			descriptionJustify="end"
			labelWidth="lg"
			align={align}
			title={
				<Text variant="pBodySmall/medium" color="steel-dark">
					{title}
				</Text>
			}
		>
			{children}
		</DescriptionItem>
	);
}

function ObjectViewCard({ children }: { children: ReactNode }) {
	return (
		<div>
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

	const [storageRebateFormatted] = useFormatCoin(storageRebate, SUI_TYPE_ARG, CoinFormat.FULL);

	const { address, module } = parseStructTag(objectType);

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

			<div className="flex h-full w-full flex-row gap-6">
				<div className="flex min-w-[50%] basis-1/2 flex-col gap-4">
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
							<ObjectLink objectId={data.data?.objectId!} />
						</ObjectViewItem>

						<ObjectViewItem title="Type" align="start">
							<ObjectLink
								label={<div className="text-right">{trimStdLibPrefix(objectType)}</div>}
								objectId={`${address}?module=${module}`}
							/>
						</ObjectViewItem>
					</ObjectViewCard>

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
				</div>

				<div className="flex min-w-[50%] basis-1/2 flex-col gap-4">
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

					{display && (display.link || display.project_url) && (
						<ObjectViewCard>
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
		</div>
	);
}
