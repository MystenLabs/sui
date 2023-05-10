// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    ReactNode,
    createContext,
    useContext,
    useEffect,
    useMemo,
    useRef,
    useState,
} from 'react';
import { initializeApp, FirebaseOptions, FirebaseApp } from 'firebase/app';
import {
    fetchAndActivate,
    getRemoteConfig,
    getValue,
    RemoteConfig,
} from 'firebase/remote-config';

export class FeatureFlags {
    fetched: boolean;
    ready: Promise<void>;

    #app: FirebaseApp;
    #remoteConfig: RemoteConfig;
    constructor(firebaseConfig: FirebaseOptions) {
        this.#app = initializeApp(firebaseConfig);
        this.#remoteConfig = getRemoteConfig(this.#app);

        // TODO: Set better value here:
        this.#remoteConfig.settings.minimumFetchIntervalMillis = 3600000;

        this.fetched = false;
        this.ready = fetchAndActivate(this.#remoteConfig).then(() => {
            this.fetched = true;
        });
    }

    getFeature(key: string) {
        return getValue(this.#remoteConfig, key);
    }
}

export const FeatureFlagsContext = createContext<FeatureFlags | null>(null);

function useFeatureFlagsContext() {
    const context = useContext(FeatureFlagsContext);
    if (!context) {
        throw new Error(
            'useFeatureFlagsContext must be used within a FeatureFlagsContext'
        );
    }
    return context;
}

export function FeatureFlagsProvider({
    featureFlags,
    children,
}: {
    featureFlags: FeatureFlags;
    children: ReactNode;
}) {
    return (
        <FeatureFlagsContext.Provider value={featureFlags}>
            {children}
        </FeatureFlagsContext.Provider>
    );
}

export function useFeature(key: string) {
    const featureFlags = useFeatureFlagsContext();
    const [value, setValue] = useState(() => featureFlags.getFeature(key));
    const fetchedRef = useRef(featureFlags.fetched);
    useEffect(() => {
        // If we had already fetched when the component mounted, we don't need to subscribe to changes:
        if (fetchedRef.current) return;

        featureFlags.ready.then(() => {
            setValue(featureFlags.getFeature(key));
        });
    }, [featureFlags]);
    return value;
}

export function useFeatureConfig(key: string) {
    const feature = useFeature(key);
    return useMemo(() => JSON.parse(feature.asString()), [feature]);
}

export function useFeatureOn(key: string) {
    const feature = useFeature(key);
    return feature.asBoolean();
}
