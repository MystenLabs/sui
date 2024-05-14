import {useFeatureIsOn} from "@growthbook/growthbook-react";
import {useResolveSuiNSName} from "../../../../../core";
import {normalizeSuiNSName} from "@mysten/sui.js/utils";

export function useAppResolveSuinsName(address?: string) {
    if (!address) return undefined;
    const enableNewSuinsFormat = useFeatureIsOn('wallet-enable-new-suins-name-format');
    const { data } = useResolveSuiNSName(address);
    return data ? normalizeSuiNSName(data, enableNewSuinsFormat ? 'at' : 'dot') : undefined;
}