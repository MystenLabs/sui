/// A generic module for role-based access control (RBAC).

module std::role {
    use std::error;
    use std::signer;

    const EROLE: u64 = 0;

    struct Role<phantom Type> has key {}

    /// Assign the role to the account. The caller must pass a witness, so is
    /// expected to be a function of the module that defines `Type`.
    public fun assign_role<Type>(to: &signer, _witness: &Type) {
        assert!(!has_role<Type>(signer::address_of(to)), error::already_exists(EROLE));
        move_to<Role<Type>>(to, Role<Type>{});
    }

    /// Revoke the role from the account. The caller must pass a witness, so is
    /// expected to be a function of the module that defines `Type`.
    public fun revoke_role<Type>(from: &signer, _witness: &Type) acquires Role {
        assert!(has_role<Type>(signer::address_of(from)), error::not_found(EROLE));
        let Role<Type>{} = move_from<Role<Type>>(signer::address_of(from));
    }

    /// Return true iff the address has the role.
    public fun has_role<Type>(addr: address): bool {
        exists<Role<Type>>(addr)
    }

    /// assert! that the account has the role.
    public fun assert_has_role<Type>(account: &signer) {
        assert!(has_role<Type>(signer::address_of(account)), error::not_found(EROLE));
    }
}
