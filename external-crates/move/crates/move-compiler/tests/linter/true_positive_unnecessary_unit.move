// tests unnecessary units in if, else, and block
module a::unnecessary_unit {
    public fun t_if(b: bool) {
        let x = 0;
        x;
        if (b) () else { x = 1 };
        x;
        if (b) {} else { x = 1 };
        x;
        if (b) { () } else { x = 1 };
        x;
        if (b) {
            // new line and comment does not suppress it
        } else { x = 1 };
        x;
    }

    public fun t_else(b: bool) {
        let x = 0;
        x;
        if (b) { x = 1 } else ();
        x;
        if (b) { x = 1 } else {};
        x;
        if (b) { x = 1 } else { () };
        x;
        if (b) { x = 1 } else {
            // new line and comment does not suppress it
        };
        x;
    }

    public fun t_block(b: bool) {
        ();
        let x = 0;
        x;
        if (b) { (); () } else { x = 1 }; // doesn't trigger if/else case
        x;
        if (b) { x = 1 } else { (); (); () }; // doesn't trigger if/else case
        x;
        {};
        { () }; // inner isn't an error but the outer is
        { (); }; // inner is an error but outer isn't
        ()
    }

    // public fun t_if_else_if(b: bool, c: bool) {
    //     let x = 0;
    //     x;
    //     if (b) { x = 1 } else if (c) {};
    // }
}
