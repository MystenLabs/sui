// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module b::m {
    use a::m::T2 as M;
    use a::n::T0 as N;

    use a::m::E2 as EM;
    use a::n::E0 as EN;

    public struct T0 {
        m: M,
        n: N,
    }

    public enum E0 {
        V0 {
            m: M,
            n: N,
            em: EM,
            en: EN,
        },
        V1 {
            em: EM,
            en: EN,
        },
        V2 {
            m: M,
            n: N,
            em: EM,
            en: EN,
        }
    }
}
