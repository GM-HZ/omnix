use super::*;

#[tokio::test]
async fn unsupported_server_request_is_explicitly_rejected() {
    let request: ServerRequest = serde_json::from_value(serde_json::json!({
        "method": "currentTime/read",
        "id": 41,
        "params": { "threadId": "thread-1" }
    }))
    .expect("current-time request");
    let (resolve_tx, mut resolve_rx) = mpsc::channel(/*buffer*/ 1);

    dispatch_server_request(request, None, &HashMap::new(), &resolve_tx).await;

    match resolve_rx.recv().await.expect("resolution") {
        ResolveRequest::Reject {
            request_id, error, ..
        } => {
            assert_eq!(request_id, RequestId::Integer(41));
            assert_eq!(error.code, -32601);
        }
        ResolveRequest::Resolve { .. } => panic!("unsupported request must be rejected"),
    }
}

#[tokio::test]
async fn command_approval_is_declined_and_emitted_as_audit_event() {
    let request: ServerRequest = serde_json::from_value(serde_json::json!({
        "method": "item/commandExecution/requestApproval",
        "id": 42,
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-1",
            "startedAtMs": 0,
            "command": "rm -rf build",
            "reason": "cleanup"
        }
    }))
    .expect("approval request");
    let (event_tx, mut event_rx) = mpsc::channel(/*buffer*/ 1);
    let sinks = HashMap::from([(
        "thread-1".to_string(),
        ActiveSink {
            turn_id: Some("turn-1".to_string()),
            sender: event_tx,
            active: Arc::new(AtomicBool::new(true)),
            active_turn_id: Arc::new(Mutex::new(Some("turn-1".to_string()))),
        },
    )]);
    let (resolve_tx, mut resolve_rx) = mpsc::channel(/*buffer*/ 1);

    dispatch_server_request(request, None, &sinks, &resolve_tx).await;

    assert_eq!(
        event_rx.recv().await,
        Some(AgentEvent::ApprovalDecided(ApprovalRequest {
            kind: ApprovalKind::CommandExecution,
            item_id: "item-1".to_string(),
            reason: Some("cleanup".to_string()),
            command: Some("rm -rf build".to_string()),
            decision: ApprovalDecision::Decline,
        }))
    );
    match resolve_rx.recv().await.expect("resolution") {
        ResolveRequest::Resolve {
            request_id, result, ..
        } => {
            assert_eq!(request_id, RequestId::Integer(42));
            assert_eq!(result, serde_json::json!({ "decision": "decline" }));
        }
        ResolveRequest::Reject { .. } => panic!("supported approval must be resolved"),
    }
}

#[tokio::test]
async fn terminal_delivery_releases_guard_before_host_reads_event() {
    let notification: ServerNotification = serde_json::from_value(serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": "thread-1",
            "turn": {
                "id": "turn-1",
                "items": [],
                "status": "completed",
                "error": null
            }
        }
    }))
    .expect("turn completion");
    let (event_tx, mut event_rx) = mpsc::channel(/*buffer*/ 1);
    let active = Arc::new(AtomicBool::new(true));
    let active_turn_id = Arc::new(Mutex::new(Some("turn-1".to_string())));
    let mut sinks = HashMap::from([(
        "thread-1".to_string(),
        ActiveSink {
            turn_id: Some("turn-1".to_string()),
            sender: event_tx,
            active: Arc::clone(&active),
            active_turn_id: Arc::clone(&active_turn_id),
        },
    )]);

    deliver_bound_notification(&mut sinks, notification).await;

    assert!(!active.load(Ordering::Acquire));
    assert_eq!(
        *active_turn_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        None
    );
    assert!(matches!(
        event_rx.recv().await,
        Some(AgentEvent::Completed(_))
    ));
}
