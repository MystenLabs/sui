// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';

import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import Icon from '_components/icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './NetworkSelector.module.scss';

const NetworkSelector = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const dispatch = useAppDispatch();
    const netWorks = useMemo(
        () =>
            Object.keys(API_ENV)
                .filter(
                    (env) =>
                        process.env.SHOW_STAGING !== 'false' ||
                        env !== API_ENV.staging
                )
                .map((itm) => ({
                    style: {
                        color: API_ENV_TO_INFO[itm as keyof typeof API_ENV]
                            .color,
                    },
                    ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
                    networkName: itm,
                })),
        []
    );

    const changeNetwork = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            const networkName = e.currentTarget.dataset.network;
            const apiEnv = API_ENV[networkName as keyof typeof API_ENV];
            dispatch(changeRPCNetwork(apiEnv));
        },
        [dispatch]
    );

    return (
        <div className={st.networkOptions}>
            <ul className={st.networkLists}>
                {netWorks.map((apiEnv) => (
                    <li
                        className={st.networkItem}
                        key={apiEnv.networkName}
                        data-network={apiEnv.networkName}
                        onClick={changeNetwork}
                    >
                        <Icon
                            icon="check2"
                            className={cl(
                                st.selectedNetwork,
                                selectedApiEnv === apiEnv.networkName &&
                                    st.networkActive
                            )}
                        />
                        <div style={apiEnv.style}>
                            <Icon
                                icon="circle-fill"
                                className={st.networkIcon}
                            />
                        </div>
                        {apiEnv.name}
                    </li>
                ))}
            </ul>
        </div>
    );
};

export default NetworkSelector;
