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
