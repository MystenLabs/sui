// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';

import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import BsIcon from '_components/bs-icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './Network.module.scss';

const NetworkSelector = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const dispatch = useAppDispatch();
    const netWorks = useMemo(
        () =>
            Object.keys(API_ENV).map((itm) => ({
                style: {
                    color: API_ENV_TO_INFO[itm as keyof typeof API_ENV].color,
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
        <div className={st['network-options']}>
            <div className={st['network-header']}>RPC NETWORK</div>
            <ul className={st['network-lists']}>
                {netWorks.map((apiEnv) => (
                    <li
                        className={st['network-item']}
                        key={apiEnv.networkName}
                        data-network={apiEnv.networkName}
                        onClick={changeNetwork}
                    >
                        <BsIcon
                            icon="check2"
                            className={cl(
                                st['selected-network'],
                                selectedApiEnv === apiEnv.networkName &&
                                    st['network-active']
                            )}
                        />
                        <div style={apiEnv.style}>
                            <BsIcon
                                icon="circle-fill"
                                className={st['network-icon']}
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
