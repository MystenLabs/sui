#[defines_primitive(vector)]
module std::vector {
    #[syntax(index)]
    public native fun vborrow<Element>(v: &vector<Element>, i: u64): &mut Element;
    #[syntax(index)]
    public native fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}
