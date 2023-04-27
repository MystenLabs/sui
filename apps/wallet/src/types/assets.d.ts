// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare module '*.png' {
    const src: string;
    export default src;
}

declare module '*.jpg' {
    const src: string;
    export default src;
}

declare module '*.jpeg' {
    const src: string;
    export default src;
}

declare module '*.gif' {
    const src: string;
    export default src;
}

declare module '*.svg' {
    import { type FC, type ComponentProps } from 'react';
    const component: FC<ComponentProps<'svg'>>;
    export default component;
}
