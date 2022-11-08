// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { useMemo, useCallback, useState, useEffect } from 'react';

import { CustomRPCInput } from './custom-rpc-input';
import {
    API_ENV_TO_INFO,
    API_ENV,
    generateActiveNetworkList,
} from '_app/ApiProvider';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeRPCNetwork } from '_redux/slices/app';

import st from './NetworkSelector.module.scss';

const NetworkSelector = () => {
    const [selectedApiEnv, customRPC] = useAppSelector(({ app }) => [
        app.apiEnv,
        app.customRPC,
    ]);
    const [showCustomRPCInput, setShowCustomRPCInput] = useState<boolean>(
        selectedApiEnv === API_ENV.customRPC
    );

    const [selectedNetworkName, setSelectedNetworkName] =
        useState<string>(selectedApiEnv);

    // change the selected network name whenever the selectedApiEnv changes
    useEffect(() => {
        setSelectedNetworkName(selectedApiEnv);
    }, [selectedApiEnv]);

    const dispatch = useAppDispatch();

    const netWorks = useMemo(() => {
        return generateActiveNetworkList().map((itm) => ({
            ...API_ENV_TO_INFO[itm as keyof typeof API_ENV],
            networkName: itm,
        }));
    }, []);

    const changeNetwork = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            const networkName = e.currentTarget.dataset.network;
            setShowCustomRPCInput(networkName === API_ENV.customRPC);
            const isEmptyCustomRpc =
                networkName === API_ENV.customRPC && !customRPC;

            setSelectedNetworkName(
                networkName && !isEmptyCustomRpc ? networkName : ''
            );

            if (isEmptyCustomRpc) {
                setShowCustomRPCInput(true);
                return;
            }
            const apiEnv = API_ENV[networkName as keyof typeof API_ENV];
            dispatch(changeRPCNetwork(apiEnv));
        },
        [customRPC, dispatch]
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
                                    selectedNetworkName ===
                                        apiEnv.networkName && st.networkActive,
                                    apiEnv.networkName === API_ENV.customRPC &&
                                        showCustomRPCInput &&
                                        st.customRpcActive
                                )}
                            />

                            {apiEnv.name}
                        </button>
                    </li>
                ))}
            </ul>
            <AnimatePresence>
                {showCustomRPCInput && (
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
