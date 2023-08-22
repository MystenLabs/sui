// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet, useLocation } from 'react-router-dom';

import Loading from '_components/loading';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

const InitializePage = () => {
	const { pathname } = useLocation();
	const checkingInitialized = useInitializedGuard(
		/^\/initialize\/backup(-imported)?(\/)?$/.test(pathname),
	);
	return (
		<PageLayout
			forceFullscreen
			className="flex flex-col flex-nowrap items-center mx-auto my-0 justify-center"
		>
			<Loading loading={checkingInitialized}>
				<Outlet />
			</Loading>
		</PageLayout>
	);
};

export default InitializePage;
