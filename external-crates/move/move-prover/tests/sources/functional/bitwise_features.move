// exclude_for: cvc5
address 0x123 {

module TestFeatures {
    use std::vector;
    use std::error;
    use std::signer;

    /// The enabled features, represented by a bitset stored on chain.
    struct Features has key {
        features: vector<u8>,
    }

    spec Features {
        pragma bv=b"0";
    }

    /// Helper to check whether a feature flag is enabled.
    fun contains(features: &vector<u8>, feature: u64): bool {
        let byte_index = feature / 8;
        let bit_mask = 1 << ((feature % 8) as u8);
        byte_index < vector::length(features) && ((*vector::borrow(features, byte_index))) & bit_mask != 0
    }

    spec contains {
        pragma bv=b"0";
        pragma opaque;
        aborts_if false;
        ensures result == ((feature / 8) < len(features) && spec_contains(features, feature));
    }

    fun is_enabled(feature: u64): bool acquires Features {
        exists<Features>(@std) &&
            contains(&borrow_global<Features>(@std).features, feature)
    }

    spec is_enabled {
        aborts_if false;
        pragma timeout = 120;
        ensures result == (
            exists<Features>(@std) // this one does not verify
                && (((feature / 8) < len(global<Features>(@std).features)
                && spec_contains(global<Features>(@std).features, feature)))
        );
    }

    fun set(features: &mut vector<u8>, feature: u64, include: bool) {
        let old_n = vector::length(features); // ghost var
        let old_features = *features; // ghost var
        let n = old_n; // ghost var

        let byte_index = feature / 8;
        let bit_mask = 1 << ((feature % 8) as u8);
        while({
            spec {
                invariant n == len(features);
                invariant n >= old_n;
                invariant byte_index < old_n ==> len(features) == old_n;
                invariant byte_index >= old_n ==> len(features) <= byte_index + 1;
                invariant forall i in 0..old_n: features[i] == old_features[i];
                invariant forall i in old_n..n: (features[i] as u8) == (0 as u8);
            };
            vector::length(features) <= byte_index
        }) {
            vector::push_back(features, 0);
            n = n + 1;
        };
        let entry = vector::borrow_mut(features, byte_index);
        if (include)
            *entry = *entry | bit_mask
        else
            *entry = *entry & (0xff ^ bit_mask)
    }

    spec set {
        pragma bv=b"0";
        pragma timeout = 120;
        aborts_if false;
        ensures feature / 8 < len(features);
        ensures include == spec_contains(features, feature);
    }

    const EFRAMEWORK_SIGNER_NEEDED: u64 = 1;

    /// Function to enable and disable features. Can only be called by a signer of @std.
    public fun change_feature_flags(framework: &signer, enable: vector<u64>, disable: vector<u64>)
    acquires Features {
        assert!(signer::address_of(framework) == @std, error::permission_denied(EFRAMEWORK_SIGNER_NEEDED));
        if (!exists<Features>(@std)) {
            move_to<Features>(framework, Features{features: vector[]})
        };
        let features = &mut borrow_global_mut<Features>(@std).features;
        let i = 0;
        let n = vector::length(&enable);
        while (i < n) {
            set(features, *vector::borrow(&enable, i), true);
            spec {
                assert spec_contains(features, enable[i]);
            };
            i = i + 1
        };
        let i = 0;
        let n = vector::length(&disable);
        while (i < n) {
            set(features, *vector::borrow(&disable, i), false);
            spec {
                assert spec_compute_feature_flag(features, disable[i]) == (0 as u8);
            };
            i = i + 1
        };
    }

    spec fun spec_compute_feature_flag(features: vector<u8>, feature: u64): u8 {
        ((int2bv((((1 as u8) << ((feature % (8 as u64)) as u64)) as u8)) as u8)
            & features[feature/8] as u8)
    }

    spec fun spec_contains(features: vector<u8>, feature: u64): bool {
        spec_compute_feature_flag(features, feature) > (0 as u8)
    }

    spec change_feature_flags {
        aborts_if signer::address_of(framework) != @std;
    }

    public fun disable_feature_flags(disable: vector<u64>) acquires Features {
        let features = &mut borrow_global_mut<Features>(@std).features;
        let i = 0;
        let n = vector::length(&disable);
        while ({
            spec {
                invariant i <= n;
                invariant forall j in 0..i: disable[j] / 8 < len(features) && !spec_contains(features, disable[j]);
            };
            i < n
        }) {
            set(features, *vector::borrow(&disable, i), false);
            spec {
                assert (disable[i] / 8 < len(features) && !spec_contains(features, disable[i]));
            };
            i = i + 1;
        };
    }

    spec disable_feature_flags {
        pragma opaque;
        pragma timeout = 120;
        modifies global<Features>(@std);
        let post features = global<Features>(@std).features;
        ensures forall i in 0..len(disable): (disable[i] / 8 < len(features) && !spec_contains(features, disable[i]));
    }

    public fun enable_feature_flags(enable: vector<u64>) acquires Features {
        let features = &mut borrow_global_mut<Features>(@std).features;
        let i = 0;
        let n = vector::length(&enable);
        while ({
            spec {
                invariant i <= n;
                invariant forall j in 0..i: enable[j] / 8 < len(features) && spec_contains(features, enable[j]);
            };
            i < n
        }) {
            set(features, *vector::borrow(&enable, i), true);
            spec {
                assert (enable[i] / 8 < len(features) && spec_contains(features, enable[i]));
            };
            i = i + 1;
        };
    }

    spec enable_feature_flags {
        pragma opaque;
        pragma timeout = 120;
        modifies global<Features>(@std);
        let post features = global<Features>(@std).features;
        ensures forall i in 0..len(enable): (enable[i] / 8 < len(features) && spec_contains(features, enable[i]));
    }

}
}
