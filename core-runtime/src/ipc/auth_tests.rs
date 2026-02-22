//! Tests for IPC authentication module.

use super::super::auth_session::*;
use super::*;
use std::time::Duration;

#[test]
fn test_constant_time_compare_equal() {
    assert!(constant_time_compare(&[1, 2, 3, 4, 5], &[1, 2, 3, 4, 5]));
}

#[test]
fn test_constant_time_compare_different() {
    assert!(!constant_time_compare(&[1, 2, 3, 4, 5], &[1, 2, 3, 4, 6]));
}

#[test]
fn test_constant_time_compare_different_lengths() {
    assert!(!constant_time_compare(&[1, 2, 3], &[1, 2, 3, 4]));
}

#[test]
fn test_constant_time_compare_empty() {
    let a: [u8; 0] = [];
    let b: [u8; 0] = [];
    assert!(constant_time_compare(&a, &b));
}

#[test]
fn test_generate_session_id_length() {
    let id = generate_session_id();
    assert_eq!(id.len(), 64);
}

#[test]
fn test_generate_session_id_unique() {
    let id1 = generate_session_id();
    let id2 = generate_session_id();
    assert_ne!(id1, id2);
}

#[test]
fn test_generate_session_id_hex() {
    let id = generate_session_id();
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_session_token() {
    let token = SessionToken("test-session-id".to_string());
    assert_eq!(token.as_str(), "test-session-id");
}

#[test]
fn test_session_token_traits() {
    let token1 = SessionToken("abc".to_string());
    let token2 = SessionToken("abc".to_string());
    let token3 = SessionToken("def".to_string());
    let cloned = token1.clone();
    assert_eq!(token1, cloned);
    assert_eq!(token1, token2);
    assert_ne!(token1, token3);
    let mut map = std::collections::HashMap::new();
    map.insert(token1, 1);
    assert_eq!(map.get(&token2), Some(&1));
}

#[test]
fn test_auth_error_display() {
    assert!(AuthError::InvalidToken.to_string().contains("Invalid"));
    assert!(AuthError::SessionNotFound.to_string().contains("not found"));
    assert!(AuthError::SessionExpired.to_string().contains("expired"));
    assert!(AuthError::NotAuthenticated.to_string().contains("required"));
    assert!(AuthError::RateLimited.to_string().contains("try again"));
    assert!(AuthError::SessionRateLimited.to_string().contains("rate limit"));
}

#[test]
fn test_rate_limiter_initial() {
    let limiter = RateLimiter::new();
    assert!(!limiter.is_rate_limited());
}

#[test]
fn test_rate_limiter_reset() {
    let limiter = RateLimiter::new();
    limiter.record_failure();
    limiter.reset();
    assert!(!limiter.is_rate_limited());
}

#[tokio::test]
async fn test_authenticate_success() {
    let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
    let result = auth.authenticate("test-token").await;
    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.as_str().len(), 64);
}

#[tokio::test]
async fn test_authenticate_wrong_token() {
    let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));
    let result = auth.authenticate("wrong-token").await;
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

#[tokio::test]
async fn test_validate_session() {
    let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
    let session = auth.authenticate("test-token").await.unwrap();
    let result = auth.validate(&session).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_invalid_session() {
    let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
    let fake_session = SessionToken("nonexistent-session-id".to_string());
    let result = auth.validate(&fake_session).await;
    assert!(matches!(result, Err(AuthError::SessionNotFound)));
}

#[tokio::test]
async fn test_session_expiration() {
    let auth = SessionAuth::new("test-token", Duration::from_millis(1));
    let session = auth.authenticate("test-token").await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    let result = auth.validate(&session).await;
    assert!(matches!(result, Err(AuthError::SessionExpired)));
}

#[tokio::test]
async fn test_cleanup_expired_sessions() {
    let auth = SessionAuth::new("test-token", Duration::from_millis(1));
    let session = auth.authenticate("test-token").await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    auth.cleanup().await;
    let result = auth.validate(&session).await;
    assert!(matches!(result, Err(AuthError::SessionNotFound)));
}

#[tokio::test]
async fn test_connection_tracking() {
    let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
    let session = auth.authenticate("test-token").await.unwrap();
    let count1 = auth.track_connection(&session).await.unwrap();
    assert_eq!(count1, 1);
    let count2 = auth.track_connection(&session).await.unwrap();
    assert_eq!(count2, 2);
    auth.release_connection(&session).await;
    let current = auth.connection_count(&session).await.unwrap();
    assert_eq!(current, 1);
}

#[tokio::test]
async fn test_rate_limiting() {
    let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));
    for _ in 0..5 {
        let _ = auth.authenticate("wrong-token").await;
    }
    let result = auth.authenticate("correct-token").await;
    assert!(matches!(result, Err(AuthError::RateLimited)));
}

#[tokio::test]
async fn test_rate_limit_reset_on_success() {
    let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));
    for _ in 0..3 {
        let _ = auth.authenticate("wrong-token").await;
    }
    let result = auth.authenticate("correct-token").await;
    assert!(result.is_ok());
    let result = auth.authenticate("correct-token").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_sessions() {
    let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
    let session1 = auth.authenticate("test-token").await.unwrap();
    let session2 = auth.authenticate("test-token").await.unwrap();
    assert_ne!(session1, session2);
    assert!(auth.validate(&session1).await.is_ok());
    assert!(auth.validate(&session2).await.is_ok());
}
