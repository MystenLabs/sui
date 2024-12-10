//# init

//# publish-and-call --call 0x42::n::make
module 0x42::n {
    public fun make(): u64 { abort 0 }
}

//# run 0x42::n::make
