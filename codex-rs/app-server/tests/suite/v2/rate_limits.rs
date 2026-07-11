use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::AddCreditsNudgeCreditType;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SendAddCreditsNudgeEmailParams;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const INVALID_REQUEST_ERROR_CODE: i64 = -32600;
const FIRST_PARTY_BACKEND_UNAVAILABLE: &str =
    "first-party Codex backend APIs are not available in this local harness build";

#[tokio::test]
async fn get_account_rate_limits_unavailable_in_local_harness_build() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_get_account_rate_limits_request().await?;
    let error = read_error_response(&mut mcp, request_id).await?;

    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, FIRST_PARTY_BACKEND_UNAVAILABLE);

    Ok(())
}

#[tokio::test]
async fn send_add_credits_nudge_email_unavailable_in_local_harness_build() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_add_credits_nudge_email_request(SendAddCreditsNudgeEmailParams {
            credit_type: AddCreditsNudgeCreditType::Credits,
        })
        .await?;
    let error = read_error_response(&mut mcp, request_id).await?;

    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, FIRST_PARTY_BACKEND_UNAVAILABLE);

    Ok(())
}

async fn read_error_response(mcp: &mut TestAppServer, request_id: i64) -> Result<JSONRPCError> {
    let error = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    Ok(error)
}
