// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { X12 } from '@mysten/icons';
import { useEffect } from 'react';
import toast from 'react-hot-toast';

import { trackEvent } from '../../../shared/plausible';
import { useNextMenuUrl } from '../components/menu/hooks';
import { ButtonOrLink } from '../shared/utils/ButtonOrLink';

const HAS_SEEN_LEDGER_NOTIFICATION_KEY = 'has-seen-ledger-notification';
const HAS_SEEN_LEDGER_NOTIFICATION_VALUE = 'true';

const LEDGER_NOTIFICATION_TOAST_ID = 'ledger-notification-toast';

// TODO: Delete this soon because imperative toasts shouldn't be used for notifications :)
export function useLedgerNotification() {
    const accountUrl = useNextMenuUrl(true, `/accounts`);
    const isLedgerNotificationEnabled = useFeatureIsOn(
        'wallet-ledger-notification-enabled'
    );

    useEffect(() => {
        const hasSeenLedgerNotificationVal = localStorage.getItem(
            HAS_SEEN_LEDGER_NOTIFICATION_KEY
        );
        const hasSeenLedgerNotification =
            hasSeenLedgerNotificationVal === HAS_SEEN_LEDGER_NOTIFICATION_VALUE;

        if (isLedgerNotificationEnabled && !hasSeenLedgerNotification) {
            // If we don't have a timeout, the toast doesn't get rendered after initial render.
            // We'll do this for now since we don't have the time to figure out what exactly is going on
            setTimeout(() => {
                toast.success(
                    <div className="flex gap-2 items-center">
                        <div className="shrink-0">
                            <ButtonOrLink
                                className="text-inherit no-underline"
                                onClick={() => {
                                    trackEvent('LedgerNotification');
                                    localStorage.setItem(
                                        HAS_SEEN_LEDGER_NOTIFICATION_KEY,
                                        HAS_SEEN_LEDGER_NOTIFICATION_VALUE
                                    );
                                    toast.remove(LEDGER_NOTIFICATION_TOAST_ID);
                                }}
                                to={accountUrl}
                            >
                                New! Tap to Connect your Ledger
                            </ButtonOrLink>
                        </div>
                        <button
                            className="w-full flex appearance-none border-0 p-0 bg-transparent cursor-pointer text-success-dark"
                            onClick={() => {
                                localStorage.setItem(
                                    HAS_SEEN_LEDGER_NOTIFICATION_KEY,
                                    HAS_SEEN_LEDGER_NOTIFICATION_VALUE
                                );
                                toast.dismiss(LEDGER_NOTIFICATION_TOAST_ID);
                            }}
                        >
                            <div className="sr-only">Dismiss notification</div>
                            <X12 />
                        </button>
                    </div>,
                    {
                        id: LEDGER_NOTIFICATION_TOAST_ID,
                        duration: Infinity,
                    }
                );
            }, 0);
        }

        return () => {
            toast.remove(LEDGER_NOTIFICATION_TOAST_ID);
        };
    }, [accountUrl, isLedgerNotificationEnabled]);
}
