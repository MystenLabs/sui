// Since it is not a OTW (because of the multiple fields), we can pack it
module a::n {
    struct N has drop { some_field: bool, some_field2: bool  }

    public fun new(): N {
        N { some_field: false, some_field2: true }
    }
}
