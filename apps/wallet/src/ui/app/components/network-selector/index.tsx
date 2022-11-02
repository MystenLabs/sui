// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';

import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import { growthbook } from '_app/experimentation/feature-gating';
import { FEATURES } from '_app/experimentation/features';
import Icon from '_components/icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './NetworkSelector.module.scss';

const EXCLUDE_STAGING =
    process.env.SHOW_STAGING !== 'false'
        ? []
        : [API_ENV.staging as keyof typeof API_ENV];

const NetworkSelector = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const dispatch = useAppDispatch();

    const netWorks = useMemo(() => {
        const excludeCustomRPC = growthbook.isOn(FEATURES.USE_CUSTOM_RPC_URL)
            ? []
            : [API_ENV.customRPC as keyof typeof API_ENV];

        const excludeNetworks = [...EXCLUDE_STAGING, ...excludeCustomRPC];

        return Object.keys(API_ENV)
            .filter(
                (env) => !excludeNetworks.includes(env as keyof typeof API_ENV)
            )
            .map((itm) => ({
                style: {
                    color: API_ENV_TO_INFO[itm as keyof typeof API_ENV].color,
                },
                ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
                networkName: itm,
            }));
    }, []);

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
