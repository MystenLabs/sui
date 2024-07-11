// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic game that depends on randomness from drand.
///
/// The quicknet chain chain of drand creates random 32 bytes every 3 seconds. This randomness is
/// verifiable in the sense that anyone can check if a given 32 bytes bytes are indeed the i-th
/// output of drand. For more details see https://drand.love/
///
/// One could implement on-chain games that need unbiasable and unpredictable randomness using drand
/// as the source of randomness. I.e., every time the game needs randomness, it receives the next 32
/// bytes from drand (whether as part of a transaction or by reading it from an existing object) and
/// follows accordingly.
///
/// However, this simplistic flow may be insecure in some cases because the blockchain is not aware
/// of the latest round of drand, and thus it may depend on randomness that is already public.
///
/// Below we design a game that overcomes this issue as following:

/// - The game is defined for a specific drand round N in the future, for example, the round that is
///   expected in 5 mins from now.
///
///   The current round for the main chain can be retrieved (off-chain) using:
///
///       curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/latest
///
///   or using the following python script:
///
///       import time
///       genesis = 1692803367
///       curr_round = (time.time() - genesis) // 3 + 1
///
///   The round in 5 mins from now will be `curr_round + 5 * 20`. Genesis is the epoch of the first
///   round as returned from:
///
///       curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info
///
/// - Anyone can *close* the game to new participants by providing drand's randomness of round N-2
///   (i.e., 1 minute before round N). The randomness of round X can be retrieved using
///
///       curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/X
///
/// - Users can join the game as long as it is not closed and receive a *ticket*.
///
/// - Anyone can *complete* the game by proving drand's randomness of round N, which is used to
///   declare the winner.
///
/// - The owner of the winning "ticket" can request a "winner ticket" and finish the game.
///
/// As long as someone is closing the game in time (or at least before round N) we have the
/// guarantee that the winner is selected using unpredictable and unbiasable randomness. Otherwise,
/// someone could wait until the randomness of round N is public, see if it could win the game and
/// if so, join the game and drive it to completion. Therefore, honest users are encouraged to close
/// the game in time.
///
/// All the external inputs needed for the following APIs can be retrieved from one of drand's
/// public APIs, e.g. using the above curl commands.
module drand::lottery {
    use drand::lib::{derive_randomness, verify_drand_signature, safe_selection};

    // === Receiver Functions ===

    public use fun delete_ticket as Ticket.delete;
    public use fun delete_game_winner as GameWinner.delete;
    public use fun ticket_game_id as Ticket.game_id;
    public use fun game_winner_game_id as GameWinner.game_id;

    // === Object Types ===

    /// Game represents a set of parameters of a single game.
    /// This game can be extended to require ticket purchase, reward winners, etc.
    public struct Game has key, store {
        id: UID,
        round: u64,
        status: u8,
        participants: u64,
        winner: Option<u64>,
    }

    /// Ticket represents a participant in a single game.
    /// Can be deconstructed only by the owner.
    public struct Ticket has key, store {
        id: UID,
        game_id: ID,
        participant_index: u64,
    }

    /// GameWinner represents a participant that won in a specific game.
    /// Can be deconstructed only by the owner.
    public struct GameWinner has key, store {
        id: UID,
        game_id: ID,
    }

    // === Error Codes ===

    #[error]
    const EGameNotInProgress: vector<u8> =
        b"Lottery has already closed.";

    #[error]
    const EGameAlreadyCompleted: vector<u8> =
        b"Lottery winner has already been selected.";

    #[error]
    const EInvalidTicket: vector<u8> =
        b"Ticket does not match lottery";

    #[error]
    const ENotWinner: vector<u8> =
        b"Not the winning ticket";

    // === Constants ===

    // Game status
    const IN_PROGRESS: u8 = 0;
    const CLOSED: u8 = 1;
    const COMPLETED: u8 = 2;

    // === Public Functions ===

    /// Create a shared-object Game.
    public fun create(round: u64, ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            round,
            status: IN_PROGRESS,
            participants: 0,
            winner: option::none(),
        };
        transfer::public_share_object(game);
    }

    /// Anyone can close the game by providing the randomness of round-2.
    public fun close(game: &mut Game, drand_sig: vector<u8>) {
        assert!(game.status == IN_PROGRESS, EGameNotInProgress);
        verify_drand_signature(drand_sig, closing_round(game.round));
        game.status = CLOSED;
    }

    /// Anyone can complete the game by providing the randomness of round.
    public fun complete(game: &mut Game, drand_sig: vector<u8>) {
        assert!(game.status != COMPLETED, EGameAlreadyCompleted);
        verify_drand_signature(drand_sig, game.round);

        game.status = COMPLETED;
        // The randomness is derived from drand_sig by passing it through sha2_256 to make it
        // uniform.
        let digest = derive_randomness(drand_sig);
        game.winner = option::some(safe_selection(game.participants, &digest));
    }

    /// Anyone can participate in the game and receive a ticket.
    public fun participate(game: &mut Game, ctx: &mut TxContext): Ticket {
        assert!(game.status == IN_PROGRESS, EGameNotInProgress);
        Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
            participant_index: {
                let index = game.participants;
                game.participants = game.participants + 1;
                index
            }
        }
    }

    /// The winner can redeem its ticket.
    public fun redeem(ticket: &Ticket, game: &Game, ctx: &mut TxContext): GameWinner {
        assert!(object::id(game) == ticket.game_id, EInvalidTicket);
        assert!(game.winner.contains(&ticket.participant_index), ENotWinner);
        GameWinner {
            id: object::new(ctx),
            game_id: ticket.game_id,
        }
    }

    /// Note that a ticket can be deleted before the game was completed.
    public fun delete_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _, participant_index: _} = ticket;
        object::delete(id);
    }

    public fun delete_game_winner(ticket: GameWinner) {
        let GameWinner { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public fun ticket_game_id(ticket: &Ticket): ID {
        ticket.game_id
    }

    public fun game_winner_game_id(winner: &GameWinner): ID {
        winner.game_id
    }

    // === Private Helpers ===

    fun closing_round(round: u64): u64 {
        round - 2
    }
}
