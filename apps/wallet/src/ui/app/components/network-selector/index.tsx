// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { useMemo, useCallback } from 'react';

import { CustomRPCInput } from './custom-rpc-input';
import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import { FEATURES } from '_app/experimentation/features';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './NetworkSelector.module.scss';

const EXCLUDE_STAGING = process.env.SHOW_STAGING !== 'false' && API_ENV.staging;

type NetworkTypes = keyof typeof API_ENV;

const NetworkSelector = () => {
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const dispatch = useAppDispatch();
    const excludeCustomRPC =
        !useFeature(FEATURES.USE_CUSTOM_RPC_URL).on && API_ENV.customRPC;
    const excludeTestnet =
        !useFeature(FEATURES.USE_TEST_NET_ENDPOINT).on && API_ENV.testNet;

    const netWorks = useMemo(() => {
        const excludedNetworks: NetworkTypes[] = [];

        if (EXCLUDE_STAGING) {
            excludedNetworks.push(EXCLUDE_STAGING);
        }

        if (excludeCustomRPC) {
            excludedNetworks.push(excludeCustomRPC);
        }

        if (excludeTestnet) {
            excludedNetworks.push(excludeTestnet);
        }

        return Object.keys(API_ENV)
            .filter(
                (env) => !excludedNetworks.includes(env as keyof typeof API_ENV)
            )
            .map((itm) => ({
                ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
                networkName: itm,
            }));
    }, [excludeCustomRPC, excludeTestnet]);

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
                        <button
                            type="button"
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
                        </button>
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
