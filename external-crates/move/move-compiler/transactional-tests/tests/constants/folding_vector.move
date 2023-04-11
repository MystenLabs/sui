//# publish
module 0x42::M {
    const B0: vector<bool> = vector[];
    const B1: vector<bool> = vector[true];
    const B2: vector<bool> = vector[true && false];
    const B3: vector<bool> = vector[true, true && false, true == true];

    const E0: bool = vector<u8>[] == vector[];
    const E1: bool = vector[0] == vector[1, 100];

}
