// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};

use sui_types::{
    authenticator_state::ActiveJwk as NativeActiveJwk,
    transaction::AuthenticatorStateUpdate as NativeAuthenticatorStateUpdateTransaction,
};

use crate::{
    consistency::ConsistentIndexCursor,
    types::{
        cursor::{JsonCursor, Page},
        epoch::Epoch,
        uint53::UInt53,
    },
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticatorStateUpdateTransaction {
    pub native: NativeAuthenticatorStateUpdateTransaction,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

pub(crate) type CActiveJwk = JsonCursor<ConsistentIndexCursor>;

/// The active JSON Web Key representing a set of public keys for an OpenID provider
struct ActiveJwk {
    native: NativeActiveJwk,
    /// The checkpoint sequence number this was viewed at.
    checkpoint_viewed_at: u64,
}

/// System transaction for updating the on-chain state used by zkLogin.
#[Object]
impl AuthenticatorStateUpdateTransaction {
    /// Epoch of the authenticator state update transaction.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx, Some(self.native.epoch), self.checkpoint_viewed_at)
            .await
            .extend()
    }

    /// Consensus round of the authenticator state update.
    async fn round(&self) -> UInt53 {
        self.native.round.into()
    }

    /// Newly active JWKs (JSON Web Keys).
    async fn new_active_jwks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CActiveJwk>,
        last: Option<u64>,
        before: Option<CActiveJwk>,
    ) -> Result<Connection<String, ActiveJwk>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some((prev, next, _, cs)) = page.paginate_consistent_indices(
            self.native.new_active_jwks.len(),
            self.checkpoint_viewed_at,
        )?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let active_jwk = ActiveJwk {
                native: self.native.new_active_jwks[c.ix].clone(),
                checkpoint_viewed_at: c.c,
            };
            connection
                .edges
                .push(Edge::new(c.encode_cursor(), active_jwk));
        }

        Ok(connection)
    }

    /// The initial version of the authenticator object that it was shared at.
    async fn authenticator_obj_initial_shared_version(&self) -> UInt53 {
        self.native
            .authenticator_obj_initial_shared_version
            .value()
            .into()
    }
}

#[Object]
impl ActiveJwk {
    /// The string (Issuing Authority) that identifies the OIDC provider.
    async fn iss(&self) -> &str {
        &self.native.jwk_id.iss
    }

    /// The string (Key ID) that identifies the JWK among a set of JWKs, (RFC 7517, Section 4.5).
    async fn kid(&self) -> &str {
        &self.native.jwk_id.kid
    }

    /// The JWK key type parameter, (RFC 7517, Section 4.1).
    async fn kty(&self) -> &str {
        &self.native.jwk.kty
    }

    /// The JWK RSA public exponent, (RFC 7517, Section 9.3).
    async fn e(&self) -> &str {
        &self.native.jwk.e
    }

    /// The JWK RSA modulus, (RFC 7517, Section 9.3).
    async fn n(&self) -> &str {
        &self.native.jwk.n
    }

    /// The JWK algorithm parameter, (RFC 7517, Section 4.4).
    async fn alg(&self) -> &str {
        &self.native.jwk.alg
    }

    /// The most recent epoch in which the JWK was validated.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx, Some(self.native.epoch), self.checkpoint_viewed_at)
            .await
            .extend()
    }
}
