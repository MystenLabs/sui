// tests for false negatives in the loop without exit lint

module a::m {
   public fun t1() {
        loop {
            if (false) break
        }
    }

    public fun t2() {
        loop {
            if (false) return
        }
    }
}
