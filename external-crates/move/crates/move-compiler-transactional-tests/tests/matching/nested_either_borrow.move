//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Either<T,U> {
        Ok(T),
        Err(U)
    }


    public fun t(o: &Either<Either<u64, bool>, Either<u64, bool>>): u64 {
        match (o) {
            Either::Ok(Either::Ok(x)) => *x,
            Either::Ok(Either::Err(true)) => 1,
            Either::Err(Either::Ok(x)) => *x,
            Either::Err(Either::Err(true)) => 1,
            x => other(x)
        }
    }

    public fun other(_x: &Either<Either<u64, bool>, Either<u64, bool>>): u64 {
        0
    }

    public fun dtor(o: Either<Either<u64, bool>, Either<u64, bool>>) {
        match (o) {
            Either::Ok(Either::Ok(_)) 
            | Either::Ok(Either::Err(_))
            | Either::Err(Either::Ok(_))
            | Either::Err(Either::Err(_)) => {}
        }
    }

    public fun run() {
        let x = Either::Ok(Either::Ok(42));
        let y = Either::Ok(Either::Err(true));
        let z = Either::Err(Either::Ok(42));
        let w = Either::Err(Either::Err(true));
        let l = Either::Ok(Either::Err(false));

        assert!(t(&x) == 42);
        assert!(t(&y) == 1);
        assert!(t(&z) == 42);
        assert!(t(&w) == 1);
        assert!(t(&l) == 0);

        x.dtor();
        y.dtor();
        z.dtor();
        w.dtor();
        l.dtor();
    }
}

//# run 0x42::m::run
