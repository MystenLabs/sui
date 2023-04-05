// script functions no longer have any built in checks outside of visibility rules

script {
    fun t0(_s: signer, _s2: signer) {
    }
}

script {
    fun t1(_s: signer, _s2: signer, _u: u64) {
    }
}

script {
    fun t2(_s: signer, _u: u64, _s2: signer) {
    }
}
