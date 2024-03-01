// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { matchRoutes, useLocation, useParams } from 'react-router-dom';
import { useNetworkContext } from '~/context';
import { Network } from '~/utils/api/DefaultRpcClient';
import { useMemo } from 'react';
import { useGetObject } from '../../../core';
import { translate } from '~/pages/object-result/ObjectResultType';

const SUISCAN_URL_MAINNET = 'https://suiscan.xyz';
const SUISCAN_URL_TESTNET = 'https://suiscan.xyz/testnet';
const SUISCAN_URL_DEVNET = 'https://suiscan.xyz/devnet';
const SUIVISION_URL_MAINNET = 'https://suivision.xyz';
const SUIVISION_URL_TESTNET = 'https://testnet.suivision.xyz';
const SUIVISION_URL_DEVNET = 'https://suivision.xyz';

enum Routes {
	object = '/object/:id',
	checkpoint = '/checkpoint/:id',
	txblock = '/txblock/:id',
	epoch = '/epoch/:id',
	address = '/address/:id',
	validator = '/validator/:id',
	validators = '/validators',
}

function useMatchPath() {
	const location = useLocation();
	const someRoutes = [
		{ path: Routes.object },
		{ path: Routes.checkpoint },
		{ path: Routes.txblock },
		{ path: Routes.epoch },
		{ path: Routes.address },
		{ path: Routes.validator },
		{ path: Routes.validators },
	];
	const matches = matchRoutes(someRoutes, location);
	return matches?.[0]?.route.path;
}

export function useRedirectUrl(isPackage?: boolean) {
	const [network] = useNetworkContext();
	const { id } = useParams();

	const matchPath = useMatchPath();
	const hasMatch = Boolean(matchPath);

	const baseUrl = useMemo(() => {
		switch (network) {
			case Network.DEVNET:
				return {
					suiscan: SUISCAN_URL_DEVNET,
					suivision: SUIVISION_URL_DEVNET,
				};
			case Network.TESTNET:
				return {
					suiscan: SUISCAN_URL_TESTNET,
					suivision: SUIVISION_URL_TESTNET,
				};
			default:
				return {
					suiscan: SUISCAN_URL_MAINNET,
					suivision: SUIVISION_URL_MAINNET,
				};
		}
	}, [network]);

	const redirectPathname = useMemo(() => {
		switch (matchPath) {
			case Routes.object:
				return {
					suiscan: `/object/${id}`,
					suivision: isPackage ? `/package/${id}` : `/object/${id}`,
				};
			case Routes.checkpoint:
				return {
					suiscan: `/checkpoint/${id}`,
					suivision: `/checkpoint/${id}`,
				};
			case Routes.txblock:
				return {
					suiscan: `/tx/${id}`,
					suivision: `/txblock/${id}`,
				};
			case Routes.epoch:
				return {
					suiscan: `/epoch/${id}`,
					suivision: `/epoch/${id}`,
				};
			case Routes.address:
				return {
					suiscan: `/address/${id}`,
					suivision: `/address/${id}`,
				};
			case Routes.validator:
				return {
					suiscan: `/validator/${id}`,
					suivision: `/validator/${id}`,
				};
			case Routes.validators:
				return {
					suiscan: `/validators`,
					suivision: `/validators`,
				};
			default: {
				return {
					suiscan: '/',
					suivision: '/',
				};
			}
		}
	}, [id, matchPath, isPackage]);

	return {
		suivisionUrl: `${baseUrl.suivision}${redirectPathname.suivision}`,
		suiscanUrl: `${baseUrl.suiscan}${redirectPathname.suiscan}`,
		hasMatch,
	};
}

function useRedirectObject() {
	const { id } = useParams();
	const { data, isError } = useGetObject(id);
	const resp = data && !isError ? translate(data) : null;
	const isPackage = resp ? resp.objType === 'Move Package' : false;

	return useRedirectUrl(isPackage);
}

export function useRedirectExplorerUrl() {
	const matchPath = useMatchPath();
	const useRedirectHook = matchPath === Routes.object ? useRedirectObject : useRedirectUrl;
	return useRedirectHook();
}
