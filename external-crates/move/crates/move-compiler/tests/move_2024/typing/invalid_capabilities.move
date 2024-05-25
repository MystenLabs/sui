module 0x42::m {

    public struct Nothing { }
    public struct Drop has drop { }
    public struct Copy has copy { }
    public struct Store has store { }

    public enum ECopy0 has copy { V(Nothing) }
    public enum ECopy1 has copy { V(Drop) }
    public enum ECopy2 has copy { V(Store) }
    public enum ECopy3 has copy {
        V0(Copy),
        V1(Nothing)
    }

    public enum EDrop0 has drop { V(Nothing) }
    public enum EDrop1 has drop { V(Copy) }
    public enum EDrop2 has drop { V(Store) }
    public enum EDrop3 has drop {
        V0(Drop),
        V1(Nothing)
    }

    public enum EStore0 has store { V(Nothing) }
    public enum EStore1 has store { V(Copy) }
    public enum EStore2 has store { V(Drop) }
    public enum EStore3 has store {
        V0(Store),
        V1(Nothing)
    }

}
