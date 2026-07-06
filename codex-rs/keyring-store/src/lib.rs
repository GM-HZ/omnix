// Stub: OS keyring disabled per slim-agent-loop design.
use std::error::Error;
use std::fmt;
use std::fmt::Debug;

#[derive(Debug)]
pub enum CredentialStoreError {
    Other(std::io::Error),
}

impl CredentialStoreError {
    pub fn new(error: std::io::Error) -> Self {
        Self::Other(error)
    }

    pub fn message(&self) -> String {
        match self {
            Self::Other(error) => error.to_string(),
        }
    }

    pub fn into_error(self) -> std::io::Error {
        match self {
            Self::Other(error) => error,
        }
    }
}

impl fmt::Display for CredentialStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CredentialStoreError {}

/// Shared credential store abstraction — OS keychain backed (stubbed).
pub trait KeyringStore: Debug + Send + Sync {
    fn load(&self, service: &str, account: &str) -> Result<Option<String>, CredentialStoreError>;
    fn save(&self, service: &str, account: &str, value: &str) -> Result<(), CredentialStoreError>;
    fn delete(&self, service: &str, account: &str) -> Result<bool, CredentialStoreError>;
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultKeyringStore;

impl KeyringStore for DefaultKeyringStore {
    fn load(&self, _service: &str, _account: &str) -> Result<Option<String>, CredentialStoreError> {
        Ok(None)
    }

    fn save(&self, _service: &str, _account: &str, _value: &str) -> Result<(), CredentialStoreError> {
        Err(CredentialStoreError::new(std::io::Error::other(
            "OS keyring disabled per slim-agent-loop design",
        )))
    }

    fn delete(&self, _service: &str, _account: &str) -> Result<bool, CredentialStoreError> {
        Ok(false)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[derive(Default, Clone, Debug)]
    pub struct MockKeyringStore {
        credentials: Arc<Mutex<HashMap<String, String>>>,
    }

    impl MockKeyringStore {
        pub fn credential(&self, account: &str) -> String {
            let guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            guard.get(account).cloned().unwrap_or_default()
        }

        pub fn saved_value(&self, account: &str) -> Option<String> {
            let guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            guard.get(account).cloned()
        }

        pub fn set_error(&self, _account: &str, _error: std::io::Error) {}

        pub fn contains(&self, account: &str) -> bool {
            let guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            guard.contains_key(account)
        }
    }

    impl KeyringStore for MockKeyringStore {
        fn load(&self, _service: &str, account: &str) -> Result<Option<String>, CredentialStoreError> {
            let guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            Ok(guard.get(account).cloned())
        }

        fn save(&self, _service: &str, account: &str, value: &str) -> Result<(), CredentialStoreError> {
            let mut guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            guard.insert(account.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, _service: &str, account: &str) -> Result<bool, CredentialStoreError> {
            let mut guard = self.credentials.lock().unwrap_or_else(|e| e.into_inner());
            Ok(guard.remove(account).is_some())
        }
    }
}
