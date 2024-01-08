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

use crate::types::{
    cursor::{Cursor, Page},
    epoch::Epoch,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticatorStateUpdateTransaction(
    pub NativeAuthenticatorStateUpdateTransaction,
);

pub(crate) type CActiveJwk = Cursor<usize>;

/// The active JSON Web Key representing a set of public keys for an OpenID provider
struct ActiveJwk(NativeActiveJwk);

/// System transaction for updating the on-chain state used by zkLogin.
#[Object]
impl AuthenticatorStateUpdateTransaction {
    /// Epoch of the authenticator state update transaction.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx.data_unchecked(), Some(self.0.epoch))
            .await
            .extend()
    }

    /// Consensus round of the authenticator state update.
    async fn round(&self) -> u64 {
        self.0.round
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
        let Some((prev, next, cs)) = page.paginate_indices(self.0.new_active_jwks.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let active_jwk = ActiveJwk(self.0.new_active_jwks[*c].clone());
            connection
                .edges
                .push(Edge::new(c.encode_cursor(), active_jwk));
        }

        Ok(connection)
    }

    /// The initial version of the authenticator object that it was shared at.
    async fn authenticator_obj_initial_shared_version(&self) -> u64 {
        self.0.authenticator_obj_initial_shared_version.value()
    }
}

#[Object]
impl ActiveJwk {
    /// The string (Issuing Authority) that identifies the OIDC provider.
    async fn iss(&self) -> &str {
        &self.0.jwk_id.iss
    }

    /// The string (Key ID) that identifies the JWK among a set of JWKs, (RFC 7517, Section 4.5).
    async fn kid(&self) -> &str {
        &self.0.jwk_id.kid
    }

    /// The JWK key type parameter, (RFC 7517, Section 4.1).
    async fn kty(&self) -> &str {
        &self.0.jwk.kty
    }

    /// The JWK RSA public exponent, (RFC 7517, Section 9.3).
    async fn e(&self) -> &str {
        &self.0.jwk.e
    }

    /// The JWK RSA modulus, (RFC 7517, Section 9.3).
    async fn n(&self) -> &str {
        &self.0.jwk.n
    }

    /// The JWK algorithm parameter, (RFC 7517, Section 4.4).
    async fn alg(&self) -> &str {
        &self.0.jwk.alg
    }

    /// The most recent epoch in which the JWK was validated.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx.data_unchecked(), Some(self.0.epoch))
            .await
            .extend()
    }
}
