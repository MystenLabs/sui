// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The Capy module. Defines the Capy type and its functions.
module capy::capy {
    use std::string::{Self, String};
    use sui::url::{Self, Url};
    use sui::event::emit;
    use sui::dynamic_object_field as dof;

    use std::hash::sha3_256 as hash;

    /// Number of meaningful genes. Also marks the length
    /// of the hash used in the application: sha3_256.
    const GENES: u64 = 32;

    /// There's a chance to apply mutation on each gene.
    /// For testing reasons mutation chance is relatively high: 5/255.
    const MUTATION_CHANCE: u8 = 250;

    /// Base path for `Capy.url` attribute. Is temporary and improves
    /// explorer / wallet display. Always points to the dev/testnet server.
    const IMAGE_URL: vector<u8> = b"https://api.capy.art/capys/";

    /// Link to the capy on the capy.art.
    const MAIN_URL: vector<u8> = b"https://capy.art/capy/";

    // ======== Types =========

    /// Internal representation of a Gene Sequence. Each Gene
    /// corresponds to an attribute. Max number of genes is `GENES`.
    public struct Genes has store, copy, drop { sequence: vector<u8> }

    /// Defines a Capy attribute. Eg: `pattern: 'panda'`
    public struct Attribute has store, copy, drop {
        name: String,
        value: String,
    }

    /// One of the possible values defined in GeneDefinition.
    /// `selector` field corresponds to the u8 value of the gene in a sequence.
    ///
    /// See `breed` function for details on usage.
    public struct Value has store, drop, copy {
        selector: u8,
        name: String
    }

    /// Holds the definitions for each gene. They are then assigned to
    /// Capys. Newborn will receive attributes available at the time.
    public struct GeneDefinition has store {
        name: String,
        values: vector<Value>
    }

    /// The Capy itself. Every Capy has its unique set of genes,
    /// as well as generation and parents. Ownable, tradeable.
    public struct Capy has key, store {
        id: UID,
        gen: u32,
        url: Url,
        link: Url,
        genes: Genes,
        dev_genes: Genes,
        item_count: u8,
        attributes: vector<Attribute>,
    }

    /// Belongs to the creator of the game. Has store, which
    /// allows building something on top of it (ie shared object with
    /// multi-access policy for managers).
    public struct CapyManagerCap has key, store { id: UID }

    /// Every capybara is registered here. Acts as a source of randomness
    /// as well as the storage for the main information about the gamestate.
    public struct CapyRegistry has key {
        id: UID,
        capy_born: u64,
        capy_hash: vector<u8>,
        genes: vector<GeneDefinition>
    }


    // ======== Events =========

    /// Event. When a new registry has been created.
    /// Marks the start of the game.
    public struct RegistryCreated has copy, drop { id: ID }

    /// Event. Emitted when a new Gene definition was added.
    /// Helps announcing new features.
    public struct GeneDefinitionAdded has copy, drop {
        name: String,
        values: vector<Value>
    }

    /// Event. When new Capy is born.
    public struct CapyBorn has copy, drop {
        id: ID,
        gen: u32,
        genes: Genes,
        dev_genes: Genes,
        attributes: vector<Attribute>,
        parent_one: Option<ID>,
        parent_two: Option<ID>,
        bred_by: address
    }

    /// Event. Emitted when a new item is added to a capy.
    public struct ItemAdded<phantom T> has copy, drop {
        capy_id: ID,
        item_id: ID
    }

    /// Event. Emitted when an item is taken off.
    public struct ItemRemoved<phantom T> has copy, drop {
        capy_id: ID,
        item_id: ID,
    }

    // ======== View Functions ========

    /// Read extra gene sequence of a Capy as `vector<u8>`.
    public fun dev_genes(self: &Capy): &vector<u8> {
        &self.dev_genes.sequence
    }

    // ======== Functions =========

    #[allow(unused_function)]
    /// Create a shared CapyRegistry and give its creator the capability
    /// to manage the game.
    fun init(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let capy_hash = hash(object::uid_to_bytes(&id));

        emit(RegistryCreated { id: object::uid_to_inner(&id) });

        transfer::public_transfer(CapyManagerCap { id: object::new(ctx) }, tx_context::sender(ctx));
        transfer::share_object(CapyRegistry {
            id,
            capy_hash,
            capy_born: 0,
            genes: vector[],
        })
    }


    // ======= Admin Functions =======

    /// This method is rather complicated.
    /// To define a new set of attributes, Admin must send it in a format:
    /// ```
    /// name = b"name of the attribute"
    /// definitions = [
    ///     [selector_u8, ...name_bytes],
    ///     [selector_u8, ...name_bytes]
    /// ]
    /// ```
    public entry fun add_gene(
        _: &CapyManagerCap,
        reg: &mut CapyRegistry,
        name: vector<u8>,
        definitions: vector<vector<u8>>,
        _ctx: &mut TxContext
    ) {
        let name = string::utf8(name);
        let values = raw_vec_to_values(definitions);

        // emit an event confirming gene addition
        emit(GeneDefinitionAdded { name: *&name, values: *&values });

        // lastly add new gene definition to the registry
       reg.genes.push_back(GeneDefinition { name, values });
    }

    /// Batch-add new Capys with predefined gene sequences.
    public fun batch(_: &CapyManagerCap, reg: &mut CapyRegistry, mut genes: vector<vector<u8>>, ctx: &mut TxContext): vector<Capy> {
        let mut capys = vector[];
        while (genes.length() > 0) {
            let sequence = genes.pop_back();
            let capy = create_capy(reg, sequence, vector[], ctx);

            capys.push_back(capy);
        };

        capys
    }

    /// Creates an attribute with the given name and a value. Should only be used for
    /// events. Is currently a friend-only feature but will be put behind a capability
    /// authorization later.
    public(package) fun create_attribute(name: vector<u8>, value: vector<u8>): Attribute {
        Attribute {
            name: string::utf8(name),
            value: string::utf8(value)
        }
    }

    /// Create a Capy with a specified gene sequence.
    /// Also allows assigning custom attributes if an App is authorized to do it.
    public(package) fun create_capy(
        reg: &mut CapyRegistry, sequence: vector<u8>, custom_attributes: vector<Attribute>, ctx: &mut TxContext
    ): Capy {
        let id = object::new(ctx);
        let genes = Genes { sequence };
        let dev_genes = Genes { sequence: hash(sequence) };

        reg.capy_born = reg.capy_born + 1;

        reg.capy_hash.append(object::uid_to_bytes(&id));
        reg.capy_hash = hash(reg.capy_hash);

        let sender = tx_context::sender(ctx);
        let mut attributes = get_attributes(&reg.genes, &genes);

        attributes.append(custom_attributes);

        emit(CapyBorn {
            id: object::uid_to_inner(&id),
            gen: 0,
            attributes: *&attributes,
            genes: *&genes,
            dev_genes: *&dev_genes,
            parent_one: option::none(),
            parent_two: option::none(),
            bred_by: sender
        });

        Capy {
            url: img_url(&id),
            link: link_url(&id),
            id,
            genes,
            dev_genes,
            attributes,
            gen: 0,
            item_count: 0,
        }
    }

    // ======= User facing functions =======

    /// Attach an Item to a Capy. Function is generic and allows any app to attach items to
    /// Capys but the total count of items has to be lower than 255.
    public entry fun add_item<T: key + store>(capy: &mut Capy, item: T) {
        emit(ItemAdded<T> {
            capy_id: object::id(capy),
            item_id: object::id(&item)
        });

        dof::add(&mut capy.id, object::id(&item), item);
    }

    /// Remove item from the Capy.
    public entry fun remove_item<T: key + store>(capy: &mut Capy, item_id: ID, ctx: &TxContext) {
        emit(ItemRemoved<T> {
            capy_id: object::id(capy),
            item_id: *&item_id
        });

        transfer::public_transfer(dof::remove<ID, T>(&mut capy.id, item_id), tx_context::sender(ctx));
    }

    /// Breed capys and keep the newborn at sender's address.
    public entry fun breed_and_keep(
        reg: &mut CapyRegistry,
        c1: &mut Capy,
        c2: &mut Capy,
        ctx: &mut TxContext
    ) {
        transfer::public_transfer(breed(reg, c1, c2, ctx), tx_context::sender(ctx))
    }

    /// Breed two Capys together. Perform a gene science algorithm and select
    /// genes for the newborn based on the parents' genes.
    public fun breed(
        reg: &mut CapyRegistry,
        c1: &mut Capy,
        c2: &mut Capy,
        ctx: &mut TxContext
    ): Capy {
        let id = object::new(ctx);

        // Update capy hash in the registry
        reg.capy_hash.append(object::uid_to_bytes(&id));

        // compute genes
        reg.capy_hash = hash(reg.capy_hash);
        let genes = compute_genes(&reg.capy_hash, &c1.genes, &c2.genes, GENES);

        // compute dev-genes
        reg.capy_hash = hash(reg.capy_hash);
        let dev_genes = compute_genes(&reg.capy_hash, &c1.genes, &c2.genes, GENES);

        let gen = if (c1.gen > c2.gen) { c1.gen } else { c2.gen } + 1;
        let attributes = get_attributes(&reg.genes, &genes);
        let sender = tx_context::sender(ctx);

        emit(CapyBorn {
            id: object::uid_to_inner(&id),
            gen,
            genes: *&genes,
            attributes: *&attributes,
            dev_genes: *&dev_genes,
            parent_one: option::some(object::id(c1)),
            parent_two: option::some(object::id(c2)),
            bred_by: sender
        });

        // Send newborn to parents.
        Capy {
            url: img_url(&id),
            link: link_url(&id),
            id,
            gen,
            genes,
            dev_genes,
            attributes,
            item_count: 0,
        }
    }

    // ======= Private and Utility functions =======

    /// Get Capy attributes from the gene sequence.
    fun get_attributes(definitions: &vector<GeneDefinition>, genes: &Genes): vector<Attribute> {
        let mut attributes = vector[];
        let (mut i, len) = (0u64, definitions.length());
        while (i < len) {
            let gene_def = &definitions[i];
            let capy_gene = &genes.sequence[i];

            let (mut j, num_options) = (0u64, gene_def.values.length());
            while (j < num_options) {
                let value = &gene_def.values[j];
                if (*capy_gene <= value.selector) {
                    attributes.push_back(Attribute {
                        name: *&gene_def.name,
                        value: *&value.name
                    });
                    break
                };
                j = j + 1;
            };
            i = i + 1;
        };

        attributes
    }

    /// Computes genes for the newborn based on the random seed r0, and parents genes
    /// The `max` parameter affects how many genes should be changed (if there are no
    /// attributes yet for the)
    fun compute_genes(r0: &vector<u8>, g1: &Genes, g2: &Genes, max: u64): Genes {
        let mut i = 0;

        let s1 = &g1.sequence;
        let s2 = &g2.sequence;
        let mut s3 = vector[];

        let r1 = derive(r0, 1); // for parent gene selection
        let r2 = derive(r0, 2); // chance of random mutation
        let r3 = derive(r0, 3); // value selector for random mutation

        while (i < max) {
            let rng = r1[i];
            let mut gene = if (lor(rng, 127)) {
                s1[i]
            } else {
                s2[i]
            };

            // There's a tiny chance that a mutation will happen.
            if (lor(r2[i], MUTATION_CHANCE)) {
                gene = r3[i];
            };

            s3.push_back(gene);
            i = i + 1;
        };

        Genes { sequence: s3 }
    }

    /// Give true or false based on the number.
    /// Used for selecting mother/father genes.
    fun lor(rng: u8, cmp: u8): bool {
        (rng > cmp)
    }

    /// Derive something from the seed. Add a derivation path as u8, and
    /// hash the result.
    fun derive(r0: &vector<u8>, path: u8): vector<u8> {
        let mut r1 = *r0;
        r1.push_back(path);
        hash(r1)
    }


    // ==== Utilities ======

    /// Transforms a vector of raw definitions:
    /// [
    ///    [127, b"red"],
    ///    [255, b"blue"],
    /// ]
    /// Into a vector of `Value`s (in order!):
    /// [
    ///    Value { selector: 127, name: String("red") },
    ///    Value { selector: 255, name: String("blue") },
    /// ]
    fun raw_vec_to_values(mut definitions: vector<vector<u8>>): vector<Value> {
        let mut result = vector[];
        definitions.reverse();
        while (definitions.length() > 0) {
            // [selector, name]
            let mut value_def = definitions.pop_back();
            // [eman, selector]
            value_def.reverse();
            let selector = value_def.pop_back();
            let mut name = vector[];
            while (value_def.length() > 0) {
                name.push_back(value_def.pop_back());
            };

            result.push_back(Value {
                selector,
                name: string::utf8(name)
            });
        };

        result
    }

    /// Construct an image URL for the capy.
    fun img_url(c: &UID): Url {
        let mut capy_url = IMAGE_URL;
        capy_url.append(sui::hex::encode(object::uid_to_bytes(c)));
        capy_url.append(b"/svg");

        url::new_unsafe_from_bytes(capy_url)
    }

    /// Construct a Url to the capy.art.
    fun link_url(c: &UID): Url {
        let mut capy_url = MAIN_URL;
        capy_url.append(sui::hex::encode(object::uid_to_bytes(c)));
        url::new_unsafe_from_bytes(capy_url)
    }

    #[test]
    fun test_raw_vec_to_values() {
        let mut definitions: vector<vector<u8>> = vector[];

        /* push [127, "red"] */ {
            let mut def = vector[];
            def.push_back(127);
            def.append(b"red");
            definitions.push_back(def);
        };

        /* push [255, "blue"] */ {
            let mut def = vector[];
            def.push_back(255);
            def.append(b"blue");
            definitions.push_back(def);
        };

        let mut values: vector<Value> = raw_vec_to_values(definitions);

        /* expect [255, blue] */ {
            let Value { selector, name } = values.pop_back();
            assert!(selector == 255, 0);
            assert!(string::bytes(&name) == &b"blue", 0);
        };

        /* expect [127, red] */ {
            let Value { selector, name } = values.pop_back();
            assert!(selector == 127, 0);
            assert!(string::bytes(&name) == &b"red", 0);
        };

        values.destroy_empty();
    }
}
