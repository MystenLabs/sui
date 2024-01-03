// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';
import { isSuiNSName, useResolveSuiNSAddress } from '@mysten/core';
import { PageLayout } from '~/components/Layout/PageLayout';

import { PageContent } from './PageContent';

import { Header } from '~/pages/id/Header';

function SuiNSHeader({ name }: { name: string }) {
	const { data: address, isLoading } = useResolveSuiNSAddress(name);

	return <Header address={address ?? name} loading={isLoading} />;
}

function SuiNSPageContent({ name }: { name: string }) {
	const { data: address } = useResolveSuiNSAddress(name);

	return <PageContent address={address ?? name} />;
}

export function Id() {
	const { id } = useParams();
	const isSuiNSAddress = isSuiNSName(id!);

	return (
		<PageLayout
			gradient={{
				size: 'md',
				content: isSuiNSAddress ? <SuiNSHeader name={id!} /> : <Header address={id!} />,
			}}
			content={isSuiNSAddress ? <SuiNSPageContent name={id!} /> : <PageContent address={id!} />}
		/>
	);
}
