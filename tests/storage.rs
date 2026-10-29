//! User stories for the storage layer, run against a real temporary directory.

use bytes::Bytes;
use lowkey::domain::{ContentType, DbName, DocName, Document, Username};
use lowkey::storage::{FsStore, Usage, UserRecord};

/// As an operator, I can restart the server without losing data, so that
/// users find their databases and documents exactly as they left them.
#[tokio::test]
async fn operator_restarts_the_server_without_losing_data() {
    let dir = tempfile::tempdir().unwrap();
    let alice: Username = "alice".parse().unwrap();
    let notes: DbName = "notes".parse().unwrap();
    let todo: DocName = "todo".parse().unwrap();
    let document = Document {
        content_type: ContentType::Json,
        body: Bytes::from_static(br#"{"or": ["you", "know"]}"#),
    };

    // First run: a user stores a document.
    {
        let store = FsStore::open(dir.path()).await.unwrap();
        let record = UserRecord {
            password_hash: "hash".into(),
        };
        store.create_user(&alice, &record).await.unwrap();
        store.create_db(&alice, &notes).await.unwrap();
        store
            .write_doc(&alice, &notes, &todo, &document)
            .await
            .unwrap();
    }

    // Second run: everything is still there.
    let store = FsStore::open(dir.path()).await.unwrap();

    assert_eq!(store.read_user(&alice).await.unwrap().password_hash, "hash");
    assert_eq!(store.list_dbs(&alice).await.unwrap(), vec![notes.clone()]);
    assert_eq!(
        store.list_docs(&alice, &notes).await.unwrap(),
        vec![todo.clone()]
    );
    assert_eq!(
        store.read_doc(&alice, &notes, &todo).await.unwrap(),
        document
    );

    let usage = store.disk_usage().await.unwrap();
    assert_eq!(
        usage.get(&alice),
        Some(&Usage {
            databases: 1,
            documents: 1,
            bytes: document.body.len() as u64,
        })
    );
}
