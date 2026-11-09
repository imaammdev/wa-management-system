//! Tenant scoping and RBAC.
//!
//! An `Actor` is whoever is making the call: a signed-in user, or an API key.
//! Both carry an `org_id`, and optionally a `customer_id` when the credential
//! is scoped down to a single agency sub-client.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Agent,
    Admin,
    Owner,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Agent => "agent",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "viewer" => Ok(Role::Viewer),
            "agent" => Ok(Role::Agent),
            "admin" => Ok(Role::Admin),
            "owner" => Ok(Role::Owner),
            other => Err(Error::validation(format!("unknown role '{other}'"))),
        }
    }

    pub fn at_least(&self, min: Role) -> bool {
        *self >= min
    }
}

/// Coarse capability set. Checked at the route boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Read conversations, messages, contacts.
    InboxRead,
    /// Reply, assign, label, close.
    InboxWrite,
    /// Create/edit contacts, segments, templates, broadcasts.
    ContentWrite,
    /// Connect numbers, manage API keys and webhooks, invite members.
    OrgAdmin,
    /// Billing, plan changes, deleting the org.
    OrgOwner,
}

pub fn role_can(role: Role, cap: Capability) -> bool {
    use Capability::*;
    match cap {
        InboxRead => role.at_least(Role::Viewer),
        InboxWrite | ContentWrite => role.at_least(Role::Agent),
        OrgAdmin => role.at_least(Role::Admin),
        OrgOwner => role == Role::Owner,
    }
}

#[derive(Debug, Clone)]
pub enum Actor {
    User {
        user_id: Uuid,
        org_id: Uuid,
        role: Role,
    },
    ApiKey {
        api_key_id: Uuid,
        org_id: Uuid,
        /// When set, the key may only touch this sub-client's data.
        customer_id: Option<Uuid>,
        scopes: Vec<String>,
    },
}

impl Actor {
    pub fn org_id(&self) -> Uuid {
        match self {
            Actor::User { org_id, .. } => *org_id,
            Actor::ApiKey { org_id, .. } => *org_id,
        }
    }

    pub fn customer_id(&self) -> Option<Uuid> {
        match self {
            Actor::User { .. } => None,
            Actor::ApiKey { customer_id, .. } => *customer_id,
        }
    }

    pub fn user_id(&self) -> Option<Uuid> {
        match self {
            Actor::User { user_id, .. } => Some(*user_id),
            Actor::ApiKey { .. } => None,
        }
    }

    pub fn api_key_id(&self) -> Option<Uuid> {
        match self {
            Actor::ApiKey { api_key_id, .. } => Some(*api_key_id),
            Actor::User { .. } => None,
        }
    }

    /// Dashboard capability check. API keys get content/inbox rights via
    /// scopes instead, so they are denied admin capabilities outright.
    pub fn require(&self, cap: Capability) -> Result<()> {
        match self {
            Actor::User { role, .. } => {
                if role_can(*role, cap) {
                    Ok(())
                } else {
                    Err(Error::Forbidden("insufficient role"))
                }
            }
            Actor::ApiKey { .. } => match cap {
                Capability::OrgAdmin | Capability::OrgOwner => {
                    Err(Error::Forbidden("api keys cannot administer the org"))
                }
                _ => Ok(()),
            },
        }
    }

    /// Scope check for the public `/v1` API.
    pub fn require_scope(&self, scope: &str) -> Result<()> {
        match self {
            // Dashboard users carry their role's full rights.
            Actor::User { .. } => Ok(()),
            Actor::ApiKey { scopes, .. } => {
                if scopes.iter().any(|s| s == scope || s == "*") {
                    Ok(())
                } else {
                    Err(Error::Forbidden("api key missing required scope"))
                }
            }
        }
    }

    /// Guards against a customer-scoped key reaching another sub-client's row.
    pub fn assert_customer_visible(&self, row_customer_id: Option<Uuid>) -> Result<()> {
        match self.customer_id() {
            None => Ok(()),
            Some(mine) if row_customer_id == Some(mine) => Ok(()),
            Some(_) => Err(Error::Forbidden("outside this key's customer scope")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering() {
        assert!(Role::Owner.at_least(Role::Admin));
        assert!(!Role::Agent.at_least(Role::Admin));
    }

    #[test]
    fn api_key_cannot_admin() {
        let a = Actor::ApiKey {
            api_key_id: Uuid::nil(),
            org_id: Uuid::nil(),
            customer_id: None,
            scopes: vec!["*".into()],
        };
        assert!(a.require(Capability::OrgAdmin).is_err());
        assert!(a.require(Capability::InboxWrite).is_ok());
    }

    #[test]
    fn customer_scope_isolates() {
        let mine = Uuid::new_v4();
        let other = Uuid::new_v4();
        let a = Actor::ApiKey {
            api_key_id: Uuid::nil(),
            org_id: Uuid::nil(),
            customer_id: Some(mine),
            scopes: vec!["messages:write".into()],
        };
        assert!(a.assert_customer_visible(Some(mine)).is_ok());
        assert!(a.assert_customer_visible(Some(other)).is_err());
        assert!(a.assert_customer_visible(None).is_err());
    }

    #[test]
    fn scope_enforced() {
        let a = Actor::ApiKey {
            api_key_id: Uuid::nil(),
            org_id: Uuid::nil(),
            customer_id: None,
            scopes: vec!["messages:read".into()],
        };
        assert!(a.require_scope("messages:read").is_ok());
        assert!(a.require_scope("messages:write").is_err());
    }
}
