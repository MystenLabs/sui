// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Check24 } from '@mysten/icons';
import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { useMemo, useState, useEffect } from 'react';
import { toast } from 'react-hot-toast';

import { CustomRPCInput } from './custom-rpc-input';
import { API_ENV_TO_INFO, generateActiveNetworkList } from '_app/ApiProvider';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeActiveNetwork } from '_redux/slices/app';
import { API_ENV } from '_src/shared/api-env';

import st from './NetworkSelector.module.scss';

const NetworkSelector = () => {
    const [activeApiEnv, activeRpcUrl] = useAppSelector(({ app }) => [
        app.apiEnv,
        app.customRPC,
    ]);
    const [isCustomRpcInputVisible, setCustomRpcInputVisible] =
        useState<boolean>(activeApiEnv === API_ENV.customRPC);
    // change the selected network name whenever the selectedApiEnv changes
    useEffect(() => {
        setCustomRpcInputVisible(
            activeApiEnv === API_ENV.customRPC && !!activeRpcUrl
        );
    }, [activeApiEnv, activeRpcUrl]);
    const dispatch = useAppDispatch();
    const netWorks = useMemo(() => {
        return generateActiveNetworkList().map((itm) => ({
            ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
            networkName: itm,
        }));
    }, []);

    return (
        <div className={st.networkOptions}>
            <ul className={st.networkLists}>
                {netWorks.map((apiEnv) => (
                    <li className={st.networkItem} key={apiEnv.networkName}>
                        <button
                            type="button"
                            onClick={async () => {
                                if (activeApiEnv === apiEnv.env) {
                                    return;
                                }
                                setCustomRpcInputVisible(
                                    apiEnv.env === API_ENV.customRPC
                                );
                                if (apiEnv.env !== API_ENV.customRPC) {
                                    try {
                                        await dispatch(
                                            changeActiveNetwork({
                                                network: {
                                                    env: apiEnv.env,
                                                    customRpcUrl: null,
                                                },
                                                store: true,
                                            })
                                        ).unwrap();
                                    } catch (e) {
                                        toast.error((e as Error).message);
                                    }
                                }
                            }}
                            className={st.networkSelector}
                        >
                            <Check24
                                className={cl(
                                    st.networkIcon,
                                    st.selectedNetwork,
                                    activeApiEnv === apiEnv.env &&
                                        st.networkActive,
                                    apiEnv.networkName === API_ENV.customRPC &&
                                        isCustomRpcInputVisible &&
                                        st.customRpcActive
                                )}
                            />

                            {apiEnv.name}
                        </button>
                    </li>
                ))}
            </ul>
            <AnimatePresence>
                {isCustomRpcInputVisible && (
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
