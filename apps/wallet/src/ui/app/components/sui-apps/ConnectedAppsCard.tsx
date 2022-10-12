// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect } from 'react';

import SuiApp, { SuiAppEmpty } from './SuiApp';
import { notEmpty } from '_helpers';
import { useAppSelector } from '_hooks';
import { thunkExtras } from '_store/thunk-extras';

import st from './Playground.module.scss';

function ConnectedDapps() {
    useEffect(() => {
        //TODO - move to action
        thunkExtras.background.sendGetPermissionRequests();
    }, []);

    const connectedApps = useAppSelector(({ permissions }) => permissions);

    const filteredApps =
        (connectedApps.initialized &&
            connectedApps?.ids
                .map((id) => {
                    const appData = connectedApps?.entities[id];
                    // if the app is not allowed, don't show it
                    if (!appData || appData.allowed !== true) return null;

                    //TODO: add a name and descriptions field to the app data
                    // use the app name if it exists, otherwise use the origin
                    // use the first part of the domain name
                    const origin = new URL(appData.origin).hostname
                        .replace('www.', '')
                        .split('.')[0];

                    const name = appData?.name || origin;
                    return {
                        name: name,
                        icon: appData?.favIcon,
                        link: appData.origin,
                        pagelink: appData.pagelink,
                        linkLabel: appData.origin.replace('https://', ''),
                        description: '',
                        id: appData.id,
                        accounts: appData.accounts,
                        permissions: appData.permissions || [],
                        createdDate: appData.createdDate,
                        responseDate: appData.responseDate,
                    };
                })
                .filter(notEmpty)) ||
        [] ||
        [];

    return (
        <>
            <div className={cl(st.container)}>
                <div className={st.desc}>
                    <div className={st.title}>
                        {filteredApps.length
                            ? `Connected apps (${filteredApps.length})`
                            : 'No APPS connected'}
                    </div>
                    Apps you connect to through the SUI wallet in this browser
                    will show up here.
                </div>

                <div className={cl(st.apps, st.appCards)}>
                    {filteredApps.length ? (
                        filteredApps.map((app, index) => (
                            <SuiApp
                                key={index}
                                {...app}
                                displaytype="card"
                                disconnect={true}
                            />
                        ))
                    ) : (
                        <>
                            <SuiAppEmpty displaytype="card" />
                            <SuiAppEmpty displaytype="card" />
                        </>
                    )}
                </div>
            </div>
        </>
    );
}

export default ConnectedDapps;
