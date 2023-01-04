// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare module '*.svg' {
    import * as React from 'react';
    export default React.FunctionComponent<
        React.ComponentProps<'svg'> & { title?: string }
    >;
}
