import { bcs } from "@mysten/sui/bcs";
export function mod() {
    function PolicyAdminCap() {
        return bcs.struct("PolicyAdminCap", ({
            id: object.UID(),
            policy: bcs.Address
        }));
    }
    function Policy() {
        return bcs.struct("Policy", ({
            id: object.UID(),
            public_key: bcs.vector(bcs.u8()),
            private_keys: table.Table(bcs.vector(bcs.u8()), bcs.vector(bcs.u8())),
            version: bcs.u8(),
            authorized: table.Table(bcs.Address, bcs.bool()),
            signature: bcs.option(bcs.vector(bcs.u8()))
        }));
    }
    return ({
        PolicyAdminCap,
        Policy
    });
}