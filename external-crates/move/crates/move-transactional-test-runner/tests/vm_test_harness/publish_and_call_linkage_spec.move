//# init

//# publish-and-call --call 0x42::n::make
module 0x42::n {
    public fun make(): u64 { 0 }
}

//# publish-and-call --location 0x2 --linkage 0x42=>0x2 -- --call 0x42::n::make
module 0x42::n {
    public fun make(): u64 { abort 0 }
}

//# run 0x42::n::make

//# run 0x42::n::make --linkage 0x42=>0x2
