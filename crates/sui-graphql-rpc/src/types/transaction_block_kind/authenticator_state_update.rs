// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, Edge},
    *,
};

use sui_types::{
    authenticator_state::ActiveJwk as NativeActiveJwk,
    transaction::AuthenticatorStateUpdate as NativeAuthenticatorStateUpdateTransaction,
};

use crate::{
    context_data::db_data_provider::{validate_cursor_pagination, PgManager},
    error::Error,
    types::epoch::Epoch,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticatorStateUpdateTransaction(
    pub NativeAuthenticatorStateUpdateTransaction,
);

struct ActiveJwk(NativeActiveJwk);

#[Object]
impl AuthenticatorStateUpdateTransaction {
    /// Epoch of the authenticator state update transaction.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Epoch> {
        ctx.data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.0.epoch)
            .await
            .extend()
    }

    /// Consensus round of the authenticator state update.
    async fn round(&self) -> u64 {
        self.0.round
    }

    /// Newly active JWKs (JSON Web Keys).
    async fn new_active_jwk_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, ActiveJwk>> {
        // TODO: make cursor opaque (currently just an offset).
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let total = self.0.new_active_jwks.len();

        let mut lo = if let Some(after) = after {
            1 + after
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'after' cursor.".to_string()))
                .extend()?
        } else {
            0
        };

        let mut hi = if let Some(before) = before {
            before
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'before' cursor.".to_string()))
                .extend()?
        } else {
            total
        };

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        }

        // If there's a `first` limit, bound the upperbound to be at most `first` away from the
        // lowerbound.
        if let Some(first) = first {
            let first = first as usize;
            if hi - lo > first {
                hi = lo + first;
            }
        }

        // If there's a `last` limit, bound the lowerbound to be at most `last` away from the
        // upperbound.  NB. This applies after we bounded the upperbound, using `first`.
        if let Some(last) = last {
            let last = last as usize;
            if hi - lo > last {
                lo = hi - last;
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        for (idx, active_jwk) in self
            .0
            .new_active_jwks
            .iter()
            .enumerate()
            .skip(lo)
            .take(hi - lo)
        {
            connection
                .edges
                .push(Edge::new(idx.to_string(), ActiveJwk(active_jwk.clone())));
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
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Epoch> {
        ctx.data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.0.epoch)
            .await
            .extend()
    }
}
