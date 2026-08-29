use subscription::{cancel_subscription, register_cancel, SubscriptionManager};

#[tokio::test]
async fn register_and_cancel_share_one_lifecycle_table() {
    let manager = SubscriptionManager::new();
    let receiver = register_cancel(&manager, 42);

    assert_eq!(manager.len(), 1);
    cancel_subscription(&manager, 42);

    assert!(receiver.await.is_ok());
    assert!(manager.is_empty());
}

#[tokio::test]
async fn clear_cancels_every_registered_display_stream() {
    let manager = SubscriptionManager::new();
    let first = register_cancel(&manager, 1);
    let second = register_cancel(&manager, 2);

    manager.clear();

    assert!(first.await.is_ok());
    assert!(second.await.is_ok());
    assert!(manager.is_empty());
}
