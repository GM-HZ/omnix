// Stub: device code login removed per slim-agent-loop design.

/// Stub struct — device code login is no longer supported.
#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval: u64,
}

/// Stub — device code login is no longer supported.
pub async fn request_device_code(
    _opts: &crate::server::ServerOptions,
) -> std::io::Result<DeviceCode> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}

/// Stub — device code login is no longer supported.
pub async fn complete_device_code_login(
    _opts: crate::server::ServerOptions,
    _device_code: DeviceCode,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}

/// Stub — device code login is no longer supported.
pub async fn run_device_code_login(
    _opts: crate::server::ServerOptions,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}
