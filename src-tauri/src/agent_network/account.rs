//! Identity issuer context: a pinned standalone instance.
//! Credentials and validity remain App-owned; no selected UI state controls this context.
use super::NetworkError;
use crate::network_diagnostics::{error_category, RequestDiagnostic};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use zeroize::Zeroizing;

pub(crate) enum AccountOperation {
    IdentityState(String),
    Identity,
    Certificate,
    Rotate,
    Challenge,
    Current,
    Jwks,
    Revoke,
    RevokeState,
}
impl AccountOperation {
    pub(crate) fn request(&self) -> Result<(reqwest::Method, String), NetworkError> {
        let (method, path) = match self {
            Self::IdentityState(device_id) => {
                let canonical = uuid::Uuid::parse_str(device_id)
                    .map_err(|_| NetworkError::new("DEVICE_ID_INVALID"))?;
                if canonical.to_string() != *device_id {
                    return Err(NetworkError::new("DEVICE_ID_INVALID"));
                }
                (
                    reqwest::Method::GET,
                    format!("/api/agent-network/identity/state?deviceId={device_id}"),
                )
            }
            Self::Identity => (
                reqwest::Method::POST,
                "/api/agent-network/identity".into(),
            ),
            Self::Certificate => (
                reqwest::Method::POST,
                "/api/agent-network/certificate".into(),
            ),
            Self::Rotate => (
                reqwest::Method::POST,
                "/api/agent-network/identity/rotate".into(),
            ),
            Self::Challenge => (
                reqwest::Method::POST,
                "/api/agent-network/challenge".into(),
            ),
            Self::Current => (
                reqwest::Method::POST,
                "/api/agent-network/identity/current".into(),
            ),
            Self::Revoke => (
                reqwest::Method::POST,
                "/api/agent-network/revoke".into(),
            ),
            Self::RevokeState => (
                reqwest::Method::POST,
                "/api/agent-network/revoke-state".into(),
            ),
            Self::Jwks => (
                reqwest::Method::GET,
                "/api/agent-network/jwks".into(),
            ),
        };
        Ok((method, path))
    }
}

fn account_body_failure(
    status: reqwest::StatusCode,
    retry_after: Option<Duration>,
    successful_response_error: NetworkError,
) -> NetworkError {
    let mut error = if status.is_success() {
        successful_response_error
    } else {
        NetworkError::cloud("NETWORK_ACCOUNT_UNAVAILABLE", status.as_u16())
    };
    error.retry_after = retry_after;
    error
}
pub(crate) async fn read_account_response_body(
    response: &mut reqwest::Response,
    diagnostic: &RequestDiagnostic,
) -> Result<Vec<u8>, NetworkError> {
    let status = response.status();
    let retry_after = crate::agent_network::reconnect::retry_after(response);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        diagnostic.failure("body", error_category(&error), Some(status.as_u16()), None);
        account_body_failure(
            status,
            retry_after,
            NetworkError::cloud("NETWORK_ACCOUNT_UNAVAILABLE", 503),
        )
    })? {
        if bytes.len() + chunk.len() > 65_536 {
            diagnostic.failure("body", "size", Some(status.as_u16()), None);
            return Err(account_body_failure(
                status,
                retry_after,
                NetworkError::new("NETWORK_ACCOUNT_RESPONSE_INVALID"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelfhostDescriptor {
    pub service_id: String,
    pub environment: String,
    pub principal_id: String,
    pub network_id: String,
    pub name: String,
    pub issuer: String,
    pub protocol: u8,
    pub capabilities: Vec<String>,
    pub root_certificate: String,
    pub jwks: Value,
}
impl SelfhostDescriptor {
    pub(crate) fn ensure_same_trust(&self, current: &Self) -> Result<(), NetworkError> {
        if self.service_id != current.service_id
            || self.environment != current.environment
            || self.principal_id != current.principal_id
            || self.network_id != current.network_id
            || self.issuer != current.issuer
            || self.root_certificate != current.root_certificate
            || self.jwks != current.jwks
        {
            return Err(NetworkError::new("NETWORK_TRUST_CHANGED"));
        }
        Ok(())
    }
    pub(crate) fn ensure_bootstrap(
        &self,
        root: &str,
        network_url: &str,
    ) -> Result<(), NetworkError> {
        if root != self.root_certificate || network_url != self.issuer {
            return Err(NetworkError::new("NETWORK_TRUST_CHANGED"));
        }
        Ok(())
    }
    pub(crate) fn validate(&self, origin: &str) -> Result<(), NetworkError> {
        if self.issuer != origin
            || self.protocol != 1
            || !["production", "development"].contains(&self.environment.as_str())
            || self.name.is_empty()
            || self.name.len() > 512
            || self.root_certificate.len() > 20000
            || self.root_certificate.is_empty()
        {
            return Err(NetworkError::new("NETWORK_DESCRIPTOR_INVALID"));
        }
        for id in [&self.service_id, &self.principal_id, &self.network_id] {
            if uuid::Uuid::parse_str(id).map(|x| x.to_string()).as_deref() != Ok(id.as_str()) {
                return Err(NetworkError::new("NETWORK_DESCRIPTOR_INVALID"));
            }
        }
        if ["tls13-mtls", "typed-rpc", "permit-v1", "return-v1"]
            .iter()
            .any(|c| !self.capabilities.iter().any(|v| v == c))
        {
            return Err(NetworkError::new("NETWORK_PROTOCOL_UNSUPPORTED"));
        }
        super::jwt::checked_jwks(self.jwks.clone())?;
        Ok(())
    }
}
pub(crate) struct SelfhostAccount {
    pub connection_id: String,
    pub descriptor: SelfhostDescriptor,
    pub active: Arc<AtomicBool>,
    pub enrollment_key: Option<Zeroizing<String>>,
}
pub(crate) type NetworkAccountSession = Arc<SelfhostAccount>;

impl SelfhostAccount {
    pub(crate) fn binding_id(&self) -> &str {
        &self.connection_id
    }
    pub(crate) fn issuer(&self) -> &str {
        &self.descriptor.issuer
    }
    pub(crate) fn principal_id(&self) -> Result<&str, NetworkError> {
        Ok(&self.descriptor.principal_id)
    }
    pub(crate) fn environment(&self) -> &str {
        &self.descriptor.environment
    }
    pub(crate) fn service_id(&self) -> Result<&str, NetworkError> {
        Ok(&self.descriptor.service_id)
    }
    pub(crate) fn has_enrollment_key(&self) -> bool {
        self.enrollment_key.is_some()
    }
    pub(crate) fn ensure_current(&self) -> Result<(), NetworkError> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(NetworkError::new("ACCOUNT_BINDING_CHANGED"))
        }
    }
    pub(crate) async fn request(
        &self,
        operation: AccountOperation,
        body: Option<Value>,
    ) -> Result<Value, NetworkError> {
        self.ensure_current()?;
        let (method, path) = operation.request()?;
        let key = if matches!(operation, AccountOperation::Identity) {
            self.enrollment_key.as_deref().map(String::as_str)
        } else {
            None
        };
        let value = request_json(&self.descriptor.issuer, method, &path, body, key).await?;
        if matches!(operation, AccountOperation::Jwks) && value != self.descriptor.jwks {
            return Err(NetworkError::new("NETWORK_TRUST_CHANGED"));
        }
        self.ensure_current()?;
        Ok(value)
    }

    pub(crate) async fn validate_instance(&self) -> Result<(), NetworkError> {
        let current = descriptor(&self.descriptor.issuer).await?;
        self.descriptor.ensure_same_trust(&current)
    }
}
pub(crate) fn origin(value: &str) -> Result<String, NetworkError> {
    let u = url::Url::parse(value.trim()).map_err(|_| NetworkError::new("NETWORK_URL_INVALID"))?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
    {
        return Err(NetworkError::new("NETWORK_URL_INVALID"));
    }
    Ok(u.origin().ascii_serialization())
}
pub(crate) async fn descriptor(value: &str) -> Result<SelfhostDescriptor, NetworkError> {
    let origin = origin(value)?;
    let value = request_json(
        &origin,
        reqwest::Method::GET,
        "/api/agent-network/descriptor",
        None,
        None,
    )
    .await?;
    let d: SelfhostDescriptor = serde_json::from_value(value)
        .map_err(|_| NetworkError::new("NETWORK_DESCRIPTOR_INVALID"))?;
    d.validate(&origin)?;
    Ok(d)
}
pub(crate) async fn request_json(
    origin: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
    key: Option<&str>,
) -> Result<Value, NetworkError> {
    #[allow(clippy::disallowed_methods)]
    let builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    let client = crate::proxy_config::build_client_with_proxy(builder)
        .map_err(|_| NetworkError::new("NETWORK_PROXY_INVALID"))?;
    let diagnostic = RequestDiagnostic::new("network-account", &method, path);
    let mut request = client.request(method, format!("{origin}{path}"));
    if let Some(key) = key {
        request = request.header("x-agenthub-key", key);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let mut response = diagnostic
        .send(request)
        .await
        .map_err(|_| NetworkError::cloud("NETWORK_ACCOUNT_UNAVAILABLE", 503))?;
    let status = response.status();
    let retry_after = super::reconnect::retry_after(&response);
    let bytes = read_account_response_body(&mut response, &diagnostic).await?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| NetworkError::cloud("NETWORK_ACCOUNT_UNAVAILABLE", status.as_u16()))?;
    if !status.is_success() || value["success"] != true {
        let mut error = NetworkError::cloud(
            value["code"]
                .as_str()
                .unwrap_or("NETWORK_ACCOUNT_UNAVAILABLE"),
            status.as_u16(),
        );
        error.retry_after = retry_after;
        return Err(error);
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| NetworkError::new("NETWORK_ACCOUNT_RESPONSE_INVALID"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> SelfhostDescriptor {
        let mut jwk = super::super::crypto::DevicePrivateKey::generate()
            .unwrap()
            .public_jwk()
            .unwrap();
        jwk["kid"] = serde_json::json!("test");
        jwk["alg"] = serde_json::json!("ES256");
        jwk["use"] = serde_json::json!("sig");
        SelfhostDescriptor {
            service_id: "00000000-0000-0000-0000-000000000001".into(),
            environment: "development".into(),
            principal_id: "00000000-0000-0000-0000-000000000002".into(),
            network_id: "00000000-0000-0000-0000-000000000003".into(),
            name: "Team".into(),
            issuer: "https://hub.example.test".into(),
            protocol: 1,
            capabilities: vec![
                "tls13-mtls".into(),
                "typed-rpc".into(),
                "permit-v1".into(),
                "return-v1".into(),
            ],
            root_certificate: "test-root".into(),
            jwks: serde_json::json!({"keys":[jwk]}),
        }
    }
    #[test]
    fn url_contains_only_https_origin_without_credentials_or_redirect_paths() {
        assert_eq!(
            origin(" https://hub.example.test/ ").unwrap(),
            "https://hub.example.test"
        );
        for url in [
            "http://hub.example.test",
            "https://user:key@hub.example.test",
            "https://hub.example.test/manage",
            "https://hub.example.test/?key=secret",
            "https://hub.example.test/#ticket=secret",
        ] {
            assert!(origin(url).is_err());
        }
    }
    #[test]
    fn identity_trust_is_stable_across_display_rename_but_not_key_or_instance_change() {
        let d = sample();
        d.validate(&d.issuer).unwrap();
        let mut rename = d.clone();
        rename.name = "Renamed".into();
        d.ensure_same_trust(&rename).unwrap();
        for field in [
            "serviceId",
            "environment",
            "principalId",
            "networkId",
            "issuer",
            "rootCertificate",
            "jwks",
        ] {
            let mut value = serde_json::to_value(&d).unwrap();
            value[field] = if field == "jwks" {
                serde_json::json!({"keys":[]})
            } else {
                serde_json::json!("changed")
            };
            let next: SelfhostDescriptor = serde_json::from_value(value).unwrap();
            assert_eq!(
                d.ensure_same_trust(&next).unwrap_err().code,
                "NETWORK_TRUST_CHANGED"
            );
        }
        d.ensure_bootstrap(&d.root_certificate, &d.issuer).unwrap();
        assert!(d.ensure_bootstrap("different-root", &d.issuer).is_err());
        assert!(d
            .ensure_bootstrap(&d.root_certificate, "https://other.test")
            .is_err());
    }
}
