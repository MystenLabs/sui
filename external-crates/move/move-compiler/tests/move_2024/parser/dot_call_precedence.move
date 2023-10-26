module a::m {

    public struct X has drop {}
    public struct Y has drop {}

    fun ximm(_: &X): &Y { abort 0 }
    fun xmut(_: &mut X): &mut Y { abort 0}
    fun xval(_: X): Y { abort 0 }

    fun yimm(_: &Y): &X { abort 0 }
    fun ymut(_: &mut Y): &mut X { abort 0}
    fun yval(_: Y): X { abort 0 }

    fun t(cond: bool) {
        let y: &mut Y = &mut X { } . xval ();
        *y = Y {};
        let _: &mut Y = (&mut X { }) . xmut ();
        let _: Y = if (cond) Y {} else X {} . xval ();
        let _: Y = (if (cond) X {} else X {}) . xval ();
        let _: X = X {} . xval () . yval();
        let _: X = if (cond) X{}.xval().yval() else Y{}.yval().xval().yval();
        let x: &mut X = &mut if (cond) X{}.xval().yval() else Y{}.yval().xval().yval();
        *x = X {};
        let _: &mut Y = (&mut if (cond) X{} else Y {} . yval ()) . xmut ();
        abort 0
    }
}
