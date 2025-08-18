// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};
use sui_types::{
    authenticator_state::ActiveJwk as NativeActiveJwk,
    transaction::AuthenticatorStateUpdate as NativeAuthenticatorStateUpdate,
};

use crate::{
    api::scalars::{cursor::JsonCursor, uint53::UInt53},
    api::types::epoch::Epoch,
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

/// System transaction for updating the on-chain state used by zkLogin.
#[derive(Clone)]
pub(crate) struct AuthenticatorStateUpdateTransaction {
    pub(crate) native: NativeAuthenticatorStateUpdate,
    pub(crate) scope: Scope,
}

/// The active JSON Web Key representing a set of public keys for an OpenID provider.
pub(crate) struct ActiveJwk {
    native: NativeActiveJwk,
    scope: Scope,
}

pub(crate) type CActiveJwk = JsonCursor<usize>;

#[Object]
impl ActiveJwk {
    /// The string (Issuing Authority) that identifies the OIDC provider.
    async fn iss(&self) -> Option<String> {
        Some(self.native.jwk_id.iss.clone())
    }

    /// The string (Key ID) that identifies the JWK among a set of JWKs, (RFC 7517, Section 4.5).
    async fn kid(&self) -> Option<String> {
        Some(self.native.jwk_id.kid.clone())
    }

    /// The JWK key type parameter, (RFC 7517, Section 4.1).
    async fn kty(&self) -> Option<String> {
        Some(self.native.jwk.kty.clone())
    }

    /// The JWK RSA public exponent, (RFC 7517, Section 9.3).
    async fn e(&self) -> Option<String> {
        Some(self.native.jwk.e.clone())
    }

    /// The JWK RSA modulus, (RFC 7517, Section 9.3).
    async fn n(&self) -> Option<String> {
        Some(self.native.jwk.n.clone())
    }

    /// The JWK algorithm parameter, (RFC 7517, Section 4.4).
    async fn alg(&self) -> Option<String> {
        Some(self.native.jwk.alg.clone())
    }

    /// The most recent epoch in which the JWK was validated.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.native.epoch))
    }
}

#[Object]
impl AuthenticatorStateUpdateTransaction {
    /// Epoch of the authenticator state update transaction.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.native.epoch))
    }

    /// Consensus round of the authenticator state update.
    async fn round(&self) -> Option<UInt53> {
        Some(self.native.round.into())
    }

    /// Newly active JWKs (JSON Web Keys).
    async fn new_active_jwks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CActiveJwk>,
        last: Option<u64>,
        before: Option<CActiveJwk>,
    ) -> Result<Connection<String, ActiveJwk>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("AuthenticatorStateUpdateTransaction", "newActiveJwks");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.new_active_jwks.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            let active_jwk = ActiveJwk {
                native: self.native.new_active_jwks[*edge.cursor].clone(),
                scope: self.scope.clone(),
            };

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), active_jwk));
        }

        Ok(conn)
    }

    /// The initial version of the authenticator object that it was shared at.
    async fn authenticator_obj_initial_shared_version(&self) -> Option<UInt53> {
        Some(self.native.authenticator_obj_initial_shared_version.into())
    }
}
