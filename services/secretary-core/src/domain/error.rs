#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Unavailable,
    ProviderUnavailable,
    PayloadTooLarge,
}
#[derive(Debug)]
pub struct CoreError(pub ErrorKind, pub &'static str);
impl CoreError {
    pub fn invalid() -> Self {
        Self(ErrorKind::InvalidInput, "invalid_input")
    }
    pub fn drift() -> Self {
        Self(ErrorKind::Conflict, "schema_drift")
    }
    pub fn upstream() -> Self {
        Self(ErrorKind::ProviderUnavailable, "provider_unavailable")
    }
}
