module Abilities::abilities {

    public struct AllAbilities has store, drop, copy, key {}

    public struct CopyKey has copy, key {}

    public struct CopyDropStore has store, drop, copy {}

    fun all_abilities(value: AllAbilities): AllAbilities {
        value
    }

    fun copy_key(value: CopyKey): CopyKey {
        value
    }

    fun copy_drop_store(value: CopyDropStore): CopyDropStore {
        value
    }
}
