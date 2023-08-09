// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import DisconnectApp from './DisconnectApp';
import { ImageIcon } from '_app/shared/image-icon';
import ExternalLink from '_components/external-link';
import { ampli } from '_src/shared/analytics/ampli';
import { getDAppUrl } from '_src/shared/utils';
import { Text } from '_src/ui/app/shared/text';

export type DAppEntry = {
	name: string;
	description: string;
	link: string;
	icon: string;
	tags: string[];
};
export type DisplayType = 'full' | 'card';

type CardViewProp = {
	name: string;
	link: string;
	icon?: string;
};

function CardView({ name, link, icon }: CardViewProp) {
	const appUrl = getDAppUrl(link);
	const originLabel = appUrl.hostname;
	return (
		<div className="bg-white flex flex-col p-3.75 box-border w-full rounded-2xl border border-gray-45 border-solid h-32 hover:bg-sui/10 hover:border-sui/30">
			<div className="flex mb-1">
				<ImageIcon src={icon || null} label={name} fallback={name} size="lg" circle />
			</div>

			<div className="flex flex-col gap-1 justify-start item-start">
				<div className="line-clamp-2 break-all">
					<Text variant="body" weight="semibold" color="gray-90">
						{name}
					</Text>
				</div>
				<Text variant="bodySmall" weight="medium" color="steel" truncate>
					{originLabel}
				</Text>
			</div>
		</div>
	);
}

type ListViewProp = {
	name: string;
	icon?: string;
	description: string;
	tags?: string[];
};

function ListView({ name, icon, description, tags }: ListViewProp) {
	return (
		<div className="bg-white flex py-3.5 px-1.25 gap-3 item-center box-border rounded hover:bg-sui/10">
			<ImageIcon src={icon || null} label={name} fallback={name} size="xl" circle />
			<div className="flex flex-col gap-1 justify-center">
				<Text variant="body" weight="semibold" color="sui-dark">
					{name}
				</Text>
				<Text variant="pSubtitle" weight="normal" color="steel-darker">
					{description}
				</Text>
				{tags?.length && (
					<div className="flex flex-wrap gap-1 mt-0.5">
						{tags?.map((tag) => (
							<div
								className="flex item-center justify-center px-1.5 py-0.5 border border-solid border-steel rounded"
								key={tag}
							>
								<Text variant="captionSmall" weight="medium" color="steel-dark">
									{tag}
								</Text>
							</div>
						))}
					</div>
				)}
			</div>
		</div>
	);
}

export interface SuiAppProps {
	name: string;
	description: string;
	link: string;
	icon: string;
	tags: string[];
	permissionID?: string;
	displayType: DisplayType;
	openAppSite?: boolean;
}

export function SuiApp({
	name,
	description,
	link,
	icon,
	tags,
	permissionID,
	displayType,
	openAppSite,
}: SuiAppProps) {
	const [showDisconnectApp, setShowDisconnectApp] = useState(false);
	const appUrl = getDAppUrl(link);

	if (permissionID && showDisconnectApp) {
		return (
			<DisconnectApp
				name={name}
				link={link}
				icon={icon}
				permissionID={permissionID}
				setShowDisconnectApp={setShowDisconnectApp}
			/>
		);
	}

	const AppDetails =
		displayType === 'full' ? (
			<ListView name={name} description={description} icon={icon} tags={tags} />
		) : (
			<CardView name={name} link={link} icon={icon} />
		);

	if (permissionID && !openAppSite) {
		return (
			<div
				className="bg-transparent cursor-pointer text-left w-full"
				onClick={() => setShowDisconnectApp(true)}
				role="button"
			>
				{AppDetails}
			</div>
		);
	}

	return (
		<ExternalLink
			href={appUrl?.toString() ?? link}
			title={name}
			className="no-underline"
			onClick={() => {
				ampli.openedApplication({ applicationName: name });
			}}
		>
			{AppDetails}
		</ExternalLink>
	);
}
