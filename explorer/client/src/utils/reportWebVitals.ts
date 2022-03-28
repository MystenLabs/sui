// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReportHandler } from 'web-vitals';

enum ReportMethod {
    console = 'console',
    request = 'request',
}

let onPerfEntry: ReportHandler;
if (process.env.REACT_APP_REPORT_VITALS === 'true') {
    switch (process.env.REACT_APP_REPORT_VITALS_METHOD) {
        case ReportMethod.console:
            // uncomment to see web vitals logs
            // onPerfEntry = console.log;
            break;
        case ReportMethod.request:
            // nothing for now
            break;
        default:
            // do nothing by default
            break;
    }
}

const reportWebVitals = () => {
    if (onPerfEntry && onPerfEntry instanceof Function) {
        import('web-vitals').then(
            ({ getCLS, getFID, getFCP, getLCP, getTTFB }) => {
                getCLS(onPerfEntry);
                getFID(onPerfEntry);
                getFCP(onPerfEntry);
                getLCP(onPerfEntry);
                getTTFB(onPerfEntry);
            }
        );
    }
};

export default reportWebVitals;
