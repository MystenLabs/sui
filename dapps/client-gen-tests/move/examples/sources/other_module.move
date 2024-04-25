module examples::other_module {
    public struct StructFromOtherModule has store { }

    public struct AddedInAnUpgrade has copy, drop, store { }

    public fun new(): StructFromOtherModule {
        StructFromOtherModule {}
    }
}