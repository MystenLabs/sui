module 1::m {
    fun main() {
        loop ();
    }
}

module 2::m {
    fun main() {
        { (loop (): ()) };
    }
}

module 3::m {
    fun main() {
        loop {
            let x = 0;
            0 + x + 0;
        };
    }
}

module 4::m {
    fun main() {
        loop {
            // TODO can probably improve this message,
            // but its different than the normal trailing case
            let _: u64 = if (true) break else break;
        }
    }
}

module 5::m {
    fun main() {
        loop {
            break;
        }
    }
}

module 6::m {
    fun main(cond: bool) {
        loop {
            if (cond) {
                break;
            } else {
                ()
            }
        }
    }
}

module 7::m {
    fun main(cond: bool) {
        loop {
            if (cond) continue else break;
        }
    }
}

module 8::m {
    fun main(cond: bool) {
        loop {
            if (cond) abort 0 else return;
        }
    }
}

module 9::m {
    fun main(cond: bool) {
        let x;
        loop {
            if (cond) {
                x = 1;
                break
            } else {
                x = 2;
                continue
            };
        };
        x;
    }
}

module 10::m {
    fun main(cond: bool) {
        loop {
            if (cond) {
                break;
            } else {
                continue;
            };
        }
    }
}

module 11::m {
    fun main(cond: bool) {
        loop {
            if (cond) {
                return;
            } else {
                abort 0;
            };
        }
    }
}
