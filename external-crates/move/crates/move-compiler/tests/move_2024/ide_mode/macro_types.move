#[allow(ide_path_autocomplete)]
module a::collection {

    public struct Content has store {
        content: Option<u64>,
    }

    public struct Collection<phantom T> {
        items: vector<Content>,
        cost: u64,
    }

    public fun do_stuff<T>(
        collection: &mut Collection<T>,
        count: u64,
    ) {
        std::u64::do!(count, |_| {
            let content = new_empty_content();
            collection.items.push_back(content);
        });
    }

    fun new_empty_content(): Content {
        Content {
            content: option::none(),
        }
    }
}


#[allow(ide_path_autocomplete)]
module std::my_macros {
    public macro fun range_do<$T, $R: drop>($start: $T, $stop: $T, $f: |$T| -> $R) {
        let mut i = $start;
        let stop = $stop;
        while (i < stop) {
            $f(i);
            i = i + 1;
        }
    }

    public macro fun do<$T, $R: drop>($stop: $T, $f: |$T| -> $R) {
        range_do!(0, $stop, $f)
    }
}

#[allow(ide_path_autocomplete)]
module std::u64 {
    public macro fun do<$R: drop>($stop: u64, $f: |u64| -> $R) {
        std::my_macros::do!($stop, $f)
    }
}
