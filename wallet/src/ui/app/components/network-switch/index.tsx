// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback, useRef } from 'react';

import NetworkSelector from './NetworkSelector';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import BsIcon from '_components/bs-icon';
import { useAppSelector, useAppDispatch, useOnClickOutside } from '_hooks';
import { setNetworkSelector } from '_redux/slices/app';

import st from './Network.module.scss';

const Network = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const showNetworkSelect = useAppSelector(({ app }) => app.showHideNetwork);
    const dispatch = useAppDispatch();

    const openNetworkSelector = useCallback(() => {
        dispatch(setNetworkSelector(showNetworkSelect));
    }, [dispatch, showNetworkSelect]);

    const netColor = useMemo(
        () =>
            selectedApiEnv
                ? { color: API_ENV_TO_INFO[selectedApiEnv].color }
                : {},
        [selectedApiEnv]
    );
    const ref = useRef<HTMLHeadingElement>(null);

    const clickOutsidehandler = () => {
        return showNetworkSelect && dispatch(setNetworkSelector(true));
    };
    useOnClickOutside(ref, clickOutsidehandler);

    return (
        <div
            className={st.networkContainer}
            ref={ref}
            onClick={openNetworkSelector}
        >
            {selectedApiEnv ? (
                <div className={st.network} style={netColor}>
                    <BsIcon icon="circle-fill" className={st.networkIcon} />
                    <span className={st.networkName}>
                        {API_ENV_TO_INFO[selectedApiEnv].name}
                    </span>
                    <BsIcon
                        icon={showNetworkSelect ? 'chevron-up' : 'chevron-down'}
                        className={cl(st.networkIcon, st.networkDropdown)}
                    />
                </div>
            ) : null}

            {showNetworkSelect && <NetworkSelector />}
        </div>
    );
};

export default Network;
