// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { useMemo, useCallback } from 'react';
import { Link } from 'react-router-dom';

import { CustomRPCInput } from './custom-rpc-input';
import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import { growthbook } from '_app/experimentation/feature-gating';
import { FEATURES } from '_app/experimentation/features';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './NetworkSelector.module.scss';

const EXCLUDE_STAGING =
    process.env.SHOW_STAGING !== 'false' &&
    (API_ENV.staging as keyof typeof API_ENV);

const NetworkSelector = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const dispatch = useAppDispatch();

    const netWorks = useMemo(() => {
        const excludeCustomRPC =
            !growthbook.isOn(FEATURES.USE_CUSTOM_RPC_URL) && API_ENV.customRPC;

        const excludeTestnet =
            !growthbook.isOn(FEATURES.USE_TEST_NET_ENDPOINT) && API_ENV.testNet;

        const excludeNetworks = [
            ...(EXCLUDE_STAGING ? [EXCLUDE_STAGING] : []),
            ...(excludeCustomRPC ? [excludeCustomRPC] : []),
            ...(excludeTestnet ? [excludeTestnet] : []),
        ];

        return Object.keys(API_ENV)
            .filter(
                (env) => !excludeNetworks.includes(env as keyof typeof API_ENV)
            )
            .map((itm) => ({
                ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
                networkName: itm,
            }));
    }, []);

    const changeNetwork = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            e.preventDefault();
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
                    <li className={st.networkItem} key={apiEnv.networkName}>
                        <Link
                            to="#"
                            data-network={apiEnv.networkName}
                            onClick={changeNetwork}
                            className={st.networkSelector}
                        >
                            <Icon
                                icon={SuiIcons.CheckFill}
                                className={cl(
                                    st.networkIcon,
                                    st.selectedNetwork,
                                    selectedApiEnv === apiEnv.networkName &&
                                        st.networkActive
                                )}
                            />

                            {apiEnv.name}
                        </Link>
                    </li>
                ))}
            </ul>
            <AnimatePresence>
                {selectedApiEnv === API_ENV.customRPC && (
                    <motion.div
                        initial={{
                            opacity: 0,
                        }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{
                            duration: 0.5,
                            ease: 'easeInOut',
                        }}
                        className={st.customRpc}
                    >
                        <CustomRPCInput />
                    </motion.div>
                )}
            </AnimatePresence>
        </div>
    );
};

export default NetworkSelector;
