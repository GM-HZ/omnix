use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ConsumeAccountRateLimitResetCreditParams;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::RequestId;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const INVALID_REQUEST_ERROR_CODE: i64 = -32600;
const FIRST_PARTY_BACKEND_UNAVAILABLE: &str =
    "first-party Codex backend APIs are not available in this local harness build";

#[tokio::test]
async fn consume_rate_limit_reset_credit_unavailable_in_local_harness_build() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = send_consume_reset_credit(&mut mcp, "request-1").await?;
    let error = read_error_response(&mut mcp, request_id).await?;

    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, FIRST_PARTY_BACKEND_UNAVAILABLE);

    Ok(())
}

#[tokio::test]
async fn consume_account_rate_limit_reset_credit_rejects_empty_idempotency_key() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = send_consume_reset_credit(&mut mcp, "").await?;
    let error = read_error_response(&mut mcp, request_id).await?;

    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, "idempotencyKey must not be empty");

    Ok(())
}

async fn send_consume_reset_credit(mcp: &mut TestAppServer, idempotency_key: &str) -> Result<i64> {
    mcp.send_consume_account_rate_limit_reset_credit_request(
        ConsumeAccountRateLimitResetCreditParams {
            idempotency_key: idempotency_key.to_string(),
        },
    )
    .await
}

async fn read_error_response(mcp: &mut TestAppServer, request_id: i64) -> Result<JSONRPCError> {
    let error = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    Ok(error)
}
