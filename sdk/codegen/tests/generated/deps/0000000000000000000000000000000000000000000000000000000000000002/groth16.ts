import { bcs } from "@mysten/sui/bcs";
export function Curve() {
    return bcs.struct("Curve", ({
        id: bcs.u8()
    }));
}
export function PreparedVerifyingKey() {
    return bcs.struct("PreparedVerifyingKey", ({
        vk_gamma_abc_g1_bytes: bcs.vector(bcs.u8()),
        alpha_g1_beta_g2_bytes: bcs.vector(bcs.u8()),
        gamma_g2_neg_pc_bytes: bcs.vector(bcs.u8()),
        delta_g2_neg_pc_bytes: bcs.vector(bcs.u8())
    }));
}
export function PublicProofInputs() {
    return bcs.struct("PublicProofInputs", ({
        bytes: bcs.vector(bcs.u8())
    }));
}
export function ProofPoints() {
    return bcs.struct("ProofPoints", ({
        bytes: bcs.vector(bcs.u8())
    }));
}