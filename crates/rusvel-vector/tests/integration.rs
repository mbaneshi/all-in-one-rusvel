use rusvel_core::ports::VectorStorePort;
use rusvel_vector::LanceVectorStore;

const DIMS: usize = 4;

fn embedding(values: [f32; DIMS]) -> Vec<f32> {
    values.to_vec()
}

#[tokio::test]
async fn upsert_and_count() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceVectorStore::open(dir.path().to_str().unwrap(), DIMS)
        .await
        .unwrap();

    assert_eq!(store.count().await.unwrap(), 0);

    store
        .upsert(
            "a",
            "hello world",
            embedding([1.0, 0.0, 0.0, 0.0]),
            serde_json::json!({"source": "test"}),
        )
        .await
        .unwrap();

    assert_eq!(store.count().await.unwrap(), 1);
}

#[tokio::test]
async fn upsert_overwrites_existing_entry() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceVectorStore::open(dir.path().to_str().unwrap(), DIMS)
        .await
        .unwrap();

    store
        .upsert(
            "x",
            "first",
            embedding([1.0, 0.0, 0.0, 0.0]),
            serde_json::json!({"source": "v1"}),
        )
        .await
        .unwrap();
    store
        .upsert(
            "x",
            "second",
            embedding([0.0, 1.0, 0.0, 0.0]),
            serde_json::json!({"source": "v2"}),
        )
        .await
        .unwrap();

    let entries = store.list(10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "second");
}

#[tokio::test]
async fn delete_removes_entry() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceVectorStore::open(dir.path().to_str().unwrap(), DIMS)
        .await
        .unwrap();

    store
        .upsert(
            "d1",
            "delete me",
            embedding([1.0, 0.0, 0.0, 0.0]),
            serde_json::json!({"source": "t"}),
        )
        .await
        .unwrap();
    assert_eq!(store.count().await.unwrap(), 1);

    store.delete("d1").await.unwrap();
    assert_eq!(store.count().await.unwrap(), 0);
}

#[tokio::test]
async fn list_returns_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceVectorStore::open(dir.path().to_str().unwrap(), DIMS)
        .await
        .unwrap();

    store
        .upsert(
            "e1",
            "alpha",
            embedding([1.0, 0.0, 0.0, 0.0]),
            serde_json::json!({"source": "a"}),
        )
        .await
        .unwrap();
    store
        .upsert(
            "e2",
            "beta",
            embedding([0.0, 1.0, 0.0, 0.0]),
            serde_json::json!({"source": "b"}),
        )
        .await
        .unwrap();

    let entries = store.list(10).await.unwrap();
    assert_eq!(entries.len(), 2);

    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"e1"));
    assert!(ids.contains(&"e2"));
}

#[tokio::test]
async fn search_returns_closest_vector() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceVectorStore::open(dir.path().to_str().unwrap(), DIMS)
        .await
        .unwrap();

    store
        .upsert(
            "s1",
            "north",
            embedding([1.0, 0.0, 0.0, 0.0]),
            serde_json::json!({"source": "t"}),
        )
        .await
        .unwrap();
    store
        .upsert(
            "s2",
            "east",
            embedding([0.0, 1.0, 0.0, 0.0]),
            serde_json::json!({"source": "t"}),
        )
        .await
        .unwrap();
    store
        .upsert(
            "s3",
            "south",
            embedding([0.0, 0.0, 1.0, 0.0]),
            serde_json::json!({"source": "t"}),
        )
        .await
        .unwrap();

    let results = store.search(&[0.9, 0.1, 0.0, 0.0], 2).await.unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].entry.id, "s1");
    assert!(results[0].score > 0.0);
}
