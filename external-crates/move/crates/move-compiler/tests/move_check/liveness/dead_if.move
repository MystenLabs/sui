module 0x42::m {
    fun t0(cond: bool) {
        loop {  
            let () = (if (cond) { break } else { continue } : ());
        }
    }
}
