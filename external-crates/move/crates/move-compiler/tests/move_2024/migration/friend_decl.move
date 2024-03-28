module a::m {

    friend a::n1;
    friend /* nested */ a::n2;
    /* stays */friend /* nested */ a::n3; // stays
    /* stays */friend/* nested */a::n4;// stays

}

module a::n1 {}
module a::n2 {}
module a::n3 {}
module a::n4 {}
