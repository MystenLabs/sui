//# init 

//# publish
module 0x42::N {
    public fun make() {}
}

//# publish --location 0x2 --linkage 0x42=>0x2
module 0x42::N {
    public fun make() { abort 0 }
}

//# run 0x42::N::make 

//# run 0x42::N::make --linkage 0x42=>0x2 
