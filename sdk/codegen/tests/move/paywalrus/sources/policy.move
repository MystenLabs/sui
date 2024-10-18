module paywalrus::policy;

use sui::table;

const EAlreadyAuthorized: u64 = 0;
const ENotAuthorized: u64 = 1;
const EInvalidVersion: u64 = 2;

const VERSION: u8 = 1;

public struct PolicyAdminCap has key, store {
    id: UID,
    policy: ID,
}

public struct Policy has key {
    id: UID,
    public_key: vector<u8>,
    private_keys: table::Table<vector<u8>, vector<u8>>,
    version: u8,
    authorized: table::Table<address, bool>,
    signature: Option<vector<u8>>,
}

public fun new_policy(
    public_key: vector<u8>,
    ctx: &mut TxContext,
): (Policy, PolicyAdminCap) {
    let policy = Policy {
        id: object::new(ctx),
        public_key,
        private_keys: table::new(ctx),
        version: VERSION,
        authorized: table::new(ctx),
        signature: option::none(),
    };

    let cap = PolicyAdminCap {
        id: object::new(ctx),
        policy: policy.id.to_inner(),
    };

    (policy, cap)
}

#[allow(lint(share_owned))]
public fun share(policy: Policy) {
    policy.validate_version();
    transfer::share_object(policy);
}

public fun policy_id(cap: &PolicyAdminCap): ID {
    cap.policy
}

public fun add_private_key(
    policy: &mut Policy,
    cap: &PolicyAdminCap,
    public_key: vector<u8>,
    private_key: vector<u8>,
) {
    policy.validate_version();
    policy.validate_cap(cap);

    policy.private_keys.add(public_key, private_key);
}

public fun add_signature(
    policy: &mut Policy,
    cap: &PolicyAdminCap,
    signature: vector<u8>,
) {
    policy.validate_version();
    policy.validate_cap(cap);

    policy.signature = option::some(signature);
}

public fun id(policy: &Policy): ID {
    policy.id.to_inner()
}

public fun create_cap(
    policy: &Policy,
    cap: &PolicyAdminCap,
    ctx: &mut TxContext,
): PolicyAdminCap {
    policy.validate_cap(cap);
    PolicyAdminCap {
        id: object::new(ctx),
        policy: policy.id.to_inner(),
    }
}

public fun destroy_cap(cap: PolicyAdminCap) {
    let PolicyAdminCap { id, .. } = cap;
    id.delete();
}

public fun is_authorized(policy: &Policy, user: address): bool {
    policy.validate_version();
    policy.authorized.contains(user)
}

public fun authorize(policy: &mut Policy, cap: &PolicyAdminCap, user: address) {
    policy.validate_cap(cap);
    policy.validate_version();

    assert!(!policy.authorized.contains(user), EAlreadyAuthorized);
    policy.authorized.add(user, true);
}

public fun deauthorize(
    policy: &mut Policy,
    cap: &PolicyAdminCap,
    user: address,
) {
    policy.validate_cap(cap);
    policy.validate_version();
    assert!(policy.authorized.contains(user), ENotAuthorized);
    policy.authorized.remove(user);
}

fun validate_version(policy: &Policy) {
    assert!(policy.version == VERSION, EInvalidVersion);
}

fun validate_cap(policy: &Policy, cap: &PolicyAdminCap) {
    assert!(cap.policy == object::borrow_id(policy), ENotAuthorized);
}
