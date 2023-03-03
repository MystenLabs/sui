// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';
import { useEffect, useMemo } from 'react';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { permissionsSelectors } from '../../redux/slices/permissions';
import Loading from '../loading';
import { type DAppEntry, SuiApp } from './SuiApp';
import { SuiAppEmpty } from './SuiAppEmpty';
import { useAppSelector } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { prepareLinkToCompare } from '_src/shared/utils';

import st from './Playground.module.scss';

const emptyArray: DAppEntry[] = [];

function ConnectedDapps() {
    const backgroundClient = useBackgroundClient();
    useEffect(() => {
        backgroundClient.sendGetPermissionRequests();
    }, [backgroundClient]);
    const ecosystemApps =
        useFeature<DAppEntry[]>(FEATURES.WALLET_DAPPS).value ?? emptyArray;
    const loading = useAppSelector(
        ({ permissions }) => !permissions.initialized
    );
    const allPermissions = useAppSelector(permissionsSelectors.selectAll);
    const connectedApps = useMemo(
        () =>
            allPermissions
                .filter(({ allowed }) => allowed)
                .map((aPermission) => {
                    const matchedEcosystemApp = ecosystemApps.find(
                        (anEcosystemApp) => {
                            const originAdj = prepareLinkToCompare(
                                aPermission.origin
                            );
                            const pageLinkAdj = aPermission.pagelink
                                ? prepareLinkToCompare(aPermission.pagelink)
                                : null;
                            const anEcosystemAppLinkAdj = prepareLinkToCompare(
                                anEcosystemApp.link
                            );
                            return (
                                originAdj === anEcosystemAppLinkAdj ||
                                pageLinkAdj === anEcosystemAppLinkAdj
                            );
                        }
                    );
                    let appNameFromOrigin = '';
                    try {
                        appNameFromOrigin = new URL(aPermission.origin).hostname
                            .replace('www.', '')
                            .split('.')[0];
                    } catch (e) {
                        // do nothing
                    }
                    return {
                        name: aPermission.name || appNameFromOrigin,
                        description: '',
                        icon: aPermission.favIcon || '',
                        link: aPermission.pagelink || aPermission.origin,
                        tags: [],
                        // override data from ecosystemApps
                        ...matchedEcosystemApp,
                        permissionID: aPermission.id,
                    };
                }),
        [allPermissions, ecosystemApps]
    );
    return (
        <Loading loading={loading}>
            <div className={cl(st.container)}>
                <div className={st.desc}>
                    <div className={st.title}>
                        {connectedApps.length
                            ? `Connected apps (${connectedApps.length})`
                            : 'No APPS connected'}
                    </div>
                    Apps you connect to through the SUI wallet in this browser
                    will show up here.
                </div>

                <div className={cl(st.apps, st.appCards)}>
                    {connectedApps.length ? (
                        connectedApps.map((app) => (
                            <SuiApp
                                key={app.permissionID}
                                {...app}
                                displayType="card"
                            />
                        ))
                    ) : (
                        <>
                            <SuiAppEmpty displayType="card" />
                            <SuiAppEmpty displayType="card" />
                        </>
                    )}
                </div>
            </div>
        </Loading>
    );
}

export default ConnectedDapps;
