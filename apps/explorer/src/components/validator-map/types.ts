// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiValidatorSummary } from "@mysten/sui.js";

export interface ValidatorWithLocation extends SuiValidatorSummary {
    ip: string;
    hostname: string;
    city: string;
    region: string;
    country: string;
    loc: string;
    postal: string;
    timezone: string;
    asn: {
        asn : string;
        name: string;
        domain : string;
        route: string;
        type: string;
    }
    countryCode: string;
    countryFlag: {
        emoji: string;
        unicode: string;
    }
    countryCurrency: {
        code: string;
        symbol: string;
    }
    continent: {
        code: string;
        name: string;
    }
    isEU: boolean;
}