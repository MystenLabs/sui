(function() {
    var type_impls = Object.fromEntries([["consensus_core",[]],["consensus_types",[]],["sui_types",[]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[21,23,17]}