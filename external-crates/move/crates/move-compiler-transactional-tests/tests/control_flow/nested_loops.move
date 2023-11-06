//# publish
module 0x42::M {
    public fun foobar(cond: bool) {
        loop {
            loop {
                if (cond) break
            };
            if (cond) break
        }
    }
}

//# run
script {
use 0x42::M;

fun main() {
    M::foobar(true)
}
}
