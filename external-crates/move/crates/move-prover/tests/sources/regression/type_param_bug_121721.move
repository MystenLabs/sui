// This is a reduced example of Vote.move that seems to have a bug.

// There are two invariants involved.

// 1. (at line 167:) Whenever Ballots<Proposal> is published at a given
//    address, BallotCounter is published at the same address.

// 2. (att line 173:) Every Ballot in the vector in the ballots field of
//    the Ballots struct published at some address has a ballot_id with a
//    counter that is less than the couner in the BallotCounter struct
//    published at the same address.

// The first invariant proves.  The second fails in create_ballot at
// line 87, at a move_to that publishes a BallotCounter with
// count 0.

// By the first invariant, there should be no Ballots structs published
// if BallotCounter is not published, so there is no way that invariant 2
// could be violated at that point (the invariant is an implication that
// checks exists<Ballots<Proposal>>(proposer_address) in the antecedant,
// so it should trivially hold).

// The bytecode after monomorphisation may show the problem.  There are 5
// instances of create_ballot in the generated boogie file.  The type
// parameter "Proposal" is instantiated in different places with #0, #1,
// and #2 (I don't know why).

// Current theory:
// Invariant 1 is assumed for #0 and #1 (but the code has #0, see below),
// and invariant 2 is assumed for #0 and #2.  But invariant 1 was never
// assumed for #2, so the Prover thinks there can be a Ballots<#2> struct
// with a vector with at least one Ballot struct.

// Before the move_to, there must have been a Ballots<#2> struct
// published with a non-empty ballots vector. Invariant 2 accesses
// globsal<BallotCounter>(addr).counter, which is undefined, so the
// assumption just forces that to be some number greater than the
// counters in the ballot ids in the vector.  When the BallotCounter is
// finally published with counter = 0, it violates the invariant.

// Maybe.

// #0, #1, #2 first show up in the bytecode dump 13 (global_invariant_analysis),
// but I'm not sure how to interpret the entrypoint info printed. It does seem to
// be instantiating @0 (first invariant?) for #0 and #1 and @1 for #0 and #2, though.

// There are some other discrepancies in the output.bpl file.  Sometimes, there
// are comments about invariants where the # parameter is different in the comment
// and in the generated code.  This makes me worry that the generated code may be
// wrong (maybe related to this error, or maybe not).

module 0x2::Bug7 {

    use std::signer;
    use std::vector;

    struct BallotID has store, copy, drop {
        counter: u64,
        proposer: address,
    }

    struct Ballot has store, copy, drop {
        // A globally unique ballot id that is created for every proposal
        ballot_id: BallotID,
    }

    struct Ballots<Proposal: store + drop> has key {
        proposal: Proposal,
        ballots: vector<Ballot>,
    }

    /// A counter which is stored under the proposers address and gets incremented
    /// everytime they create a new ballot. This is used for creating unique
    /// global identifiers for Ballots
    struct BallotCounter has key {
        counter: u64,
    }

    /// Create a ballot under the signer's address and return the `BallotID`
    public fun create_ballot<Proposal: store + copy + drop>(
        ballot_account: &signer,
        proposal: Proposal,
    ): BallotID acquires Ballots, BallotCounter {
        let ballot_address = signer::address_of(ballot_account);

        if (!exists<BallotCounter>(ballot_address)) {
            move_to(ballot_account, BallotCounter {
                counter: 0,
            });
            // DD Debug
            spec { assert !exists<Ballots<Proposal>>(ballot_address); };
        };
        if (!exists<Ballots<Proposal>>(ballot_address)) {
            move_to(ballot_account, Ballots<Proposal> {
                proposal,
                ballots: vector::empty(),
            });
        };

        let ballot_data = borrow_global_mut<Ballots<Proposal>>(ballot_address);

        let ballots = &mut ballot_data.ballots;

        let ballot_id = new_ballot_id(incr_counter(ballot_account), ballot_address);
        let ballot = Ballot {
            ballot_id: *&ballot_id,
        };
        vector::push_back(ballots, *&ballot);
        ballot_id
    }

    /// A constructor for BallotID
    public fun new_ballot_id(
        counter: u64,
        proposer: address,
    ): BallotID {
        BallotID {
            counter,
            proposer,
        }
    }

    /// incr_counter increments the counter stored under the signer's
    /// account
    fun incr_counter(account: &signer): u64 acquires BallotCounter {
        let addr = signer::address_of(account);
        let counter = &mut borrow_global_mut<BallotCounter>(addr).counter;
        let count = *counter;
        *counter = *counter + 1;
        count
    }


    ///****************************************************************
    /// Specs
    ///****************************************************************

    /// Get the ballots vector from published Ballots<Proposal>
    /// CAUTION: Returns an arbitrary value if no Ballots<Proposal> is publised at ballot_address.
    spec fun get_ballots<Proposal>(ballot_address: address): vector<Ballot> {
       global<Ballots<Proposal>>(ballot_address).ballots
    }

    /// Get the ballot matching ballot_id out of the ballots vector, if it is there.
    /// CAUTION: Returns a arbitrary value if it's not there.
    spec fun get_ballot<Proposal>(ballot_address: address, ballot_id: BallotID): Ballot {
         let ballots = global<Ballots<Proposal>>(ballot_address).ballots;
         get_ballots<Proposal>(ballot_address)[choose min i in 0..len(ballots) where ballots[i].ballot_id == ballot_id]
     }

    // Lower-level invariants

    // helper functions

    spec fun existing_ballots_have_small_counters<Proposal>(proposer_address: address): bool {
        // Just return true if there is no Ballots<Proposal> published at proposer_address
        // DD: Is it hinky to be doing this let when we don't know if it exists?
        let ballots = get_ballots<Proposal>(proposer_address);
        exists<Ballots<Proposal>>(proposer_address)
        ==> (forall i in 0..len(ballots):
                ballots[i].ballot_id.counter < global<BallotCounter>(proposer_address).counter)
    }

    spec module {
        /// Whenever there is a Ballots<Proposal> at ballot_address, there is
        /// a BallotCounter there.
        invariant<Proposal> forall ballot_address: address
            where exists<Ballots<Proposal>>(ballot_address):
                exists<BallotCounter>(ballot_address);

        /// counter values in ballots are all less than the value of the BallotCounter
        /// See note on spec fun existing_ballots_have_small_counters
        invariant<Proposal> forall addr: address: existing_ballots_have_small_counters<Proposal>(addr);
    }
}
