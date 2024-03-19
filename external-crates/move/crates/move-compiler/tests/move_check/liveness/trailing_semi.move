module 1::m {
    fun main() {
        return;
    }
}

module 2::m {
    fun main() {
        abort 0;
    }
}

module 3::m {
    fun main() {
        { return };
    }
}

module 4::m {
    fun main() {
        { abort 0 };
    }
}


module 5::m {
    fun main(cond: bool) {
        if (cond) {
            return;
        } else {
            ()
        }
    }
}

module 6::m {
    fun main(cond: bool) {
        {
            if (cond) {
                return
            } else {
                abort 0
            };
        }
    }
}

module 7::m {
    fun main(cond: bool) {
        {
            if (cond) {
                abort 0;
            } else {
                return;
            };
        }
    }
}

