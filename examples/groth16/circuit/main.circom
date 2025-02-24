pragma circom 2.1.5;

template Main() {
    signal input a;
    signal input b;
    signal output c;

    c <== a * b;
}
component main = Main();