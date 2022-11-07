// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { useIntl } from 'react-intl';

import Alert from '_components/alert';
import { useAppSelector, useFormatCoin } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { AlertProps } from '_components/alert';
import type { IntlShape } from 'react-intl';

const ONE_MINUTE = 60;
const ONE_HOUR = ONE_MINUTE * 60;
const ONE_DAY = ONE_HOUR * 24;

function formatError(
    intl: IntlShape,
    status: number,
    statusTxt: string | undefined,
    retryAfter: number | undefined
) {
    if (status === 429) {
        let retryTxt = 'later';
        if (retryAfter) {
            let unit = 'second';
            let value = retryAfter;
            if (retryAfter > ONE_DAY) {
                unit = 'day';
                value = Math.floor(retryAfter / ONE_DAY);
            } else if (retryAfter > ONE_HOUR) {
                unit = 'hour';
                value = Math.floor(retryAfter / ONE_HOUR);
            } else if (retryAfter > ONE_MINUTE) {
                unit = 'minute';
                value = Math.floor(retryAfter / ONE_MINUTE);
            }
            retryTxt = intl.formatNumber(value, {
                style: 'unit',
                unit,
                unitDisplay: 'long',
            });
        }
        return `Request limit reached, please try again after ${retryTxt}.`;
    }
    return `Gas request failed${statusTxt ? `, ${statusTxt}` : ''}.`;
}

export type FaucetMessageInfoProps = {
    className?: string;
};

function FaucetMessageInfo({ className }: FaucetMessageInfoProps) {
    const { loading, lastRequest } = useAppSelector(({ faucet }) => faucet);
    const visible = loading || !!lastRequest;
    const mode: AlertProps['mode'] = loading
        ? 'loading'
        : lastRequest?.error
        ? 'warning'
        : 'success';
    const intl = useIntl();
    const [coinsReceivedFormatted, coinsReceivedSymbol] = useFormatCoin(
        lastRequest?.totalGasReceived,
        GAS_TYPE_ARG
    );
    return (
        <AnimatePresence>
            {visible ? (
                <motion.div
                    initial={{
                        opacity: 0,
                    }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{
                        duration: 0.3,
                        ease: 'easeInOut',
                    }}
                    className={cl(className)}
                >
                    <Alert mode={mode}>
                        {loading ? 'Request in progress' : null}
                        {lastRequest?.error
                            ? formatError(
                                  intl,
                                  lastRequest.status,
                                  lastRequest.statusTxt,
                                  lastRequest.retryAfter
                              )
                            : null}
                        {lastRequest?.error === false
                            ? `${
                                  lastRequest.totalGasReceived
                                      ? `${coinsReceivedFormatted} `
                                      : ''
                              }${coinsReceivedSymbol} received`
                            : null}
                    </Alert>
                </motion.div>
            ) : null}
        </AnimatePresence>
    );
}

export default FaucetMessageInfo;
