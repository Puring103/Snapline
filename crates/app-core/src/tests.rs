use super::AppCore;
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;

#[test]
fn bootstrap_starts_with_a_blank_draft_note() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);

    let state = core.bootstrap().unwrap();

    assert!(state.notes.is_empty());
    assert_eq!(state.current.title, "Untitled");
    assert!(!state.current.pinned);
}

#[test]
fn bootstrap_uses_most_recent_saved_note_as_current() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();
    core.save_note(&note.id, "Recent", "Body", false).unwrap();

    let state = core.bootstrap().unwrap();

    assert_eq!(state.notes.len(), 1);
    assert_eq!(state.current.id, note.id);
    assert_eq!(state.current.title, "Recent");
}

#[test]
fn bootstrap_does_not_persist_a_blank_draft_note() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

    let state = core.bootstrap().unwrap();
    let repo = NoteRepository::open(&paths.db_path).unwrap();

    assert_eq!(state.current.title, "Untitled");
    assert!(repo.list_recent(10).unwrap().is_empty());
}

#[test]
fn stores_and_loads_open_shortcut() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

    core.set_open_shortcut("Ctrl+Alt+S").unwrap();

    let reopened = AppCore::open(paths).unwrap();
    assert_eq!(reopened.get_open_shortcut().unwrap(), "Ctrl+Alt+S");
}

#[test]
fn saves_png_asset_under_note_directory() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();

    let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

    assert!(asset
        .markdown_path
        .starts_with(&format!("assets/notes/{}/", note.id)));
    assert!(dir.path().join(&asset.markdown_path).exists());
    assert!(asset.asset_url.starts_with("asset://localhost/"));
}

#[test]
fn resolves_asset_urls_without_frontend_path_api() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);

    let resolved = core.resolve_asset_url("assets/notes/example/image.png");
    assert!(resolved.starts_with("asset://localhost/"));
    assert!(resolved.ends_with("image.png"));
}

#[test]
fn anonymous_save_does_not_enqueue_sync_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();

    core.save_note(&note.id, "Title", "# Title", false).unwrap();

    assert!(core.pending_sync_changes().is_err());
}

#[test]
fn account_save_enqueues_upsert_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    let note = core.create_note().unwrap();

    core.save_note(&note.id, "Title", "# Title", false).unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
    assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
}

#[test]
fn set_note_title_enqueues_upsert_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    let note = core.create_note().unwrap();
    core.save_note(&note.id, "Title", "# Title", false).unwrap();
    for change in core.pending_sync_changes().unwrap() {
        core.delete_sync_change(&change.id).unwrap();
    }

    core.set_note_title(&note.id, "Renamed").unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
    assert_eq!(changes[0].base_version, 0);
}

#[test]
fn save_png_asset_enqueues_asset_upload() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    let note = core.create_note().unwrap();

    core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::AssetUpload);
}

#[test]
fn import_snapshot_applies_notes_and_updates_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    let mut note = snapline_domain::Note::draft(chrono::Utc::now());
    note.owner_account_id = Some("acct_a".to_string());
    note.title = "Remote".to_string();
    note.server_version = 4;

    core.import_snapshot(&[note.clone()], 9).unwrap();

    assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
    assert_eq!(core.sync_state().unwrap().server_cursor, 9);
}

#[test]
fn import_snapshot_creates_conflict_copy_for_pending_local_changes() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    let note = core.create_note().unwrap();
    core.save_note(&note.id, "Local", "# Local", false).unwrap();
    let mut remote_note = note.clone();
    remote_note.title = "Remote".to_string();
    remote_note.content_md = "# Remote".to_string();
    remote_note.server_version = 3;

    core.import_snapshot(&[remote_note], 9).unwrap();

    assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
    assert!(core
        .bootstrap()
        .unwrap()
        .notes
        .iter()
        .any(|note| note.is_conflict_copy));
    assert!(core.pending_sync_changes().unwrap().is_empty());
    assert_eq!(core.sync_state().unwrap().server_cursor, 9);
}

#[test]
fn save_remote_asset_uses_metadata_location() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);
    let note_id = snapline_domain::NoteId::new();
    let asset = snapline_domain::AssetMetadata {
        id: snapline_domain::AssetId::new(),
        note_id: note_id.clone(),
        content_type: "image/png".to_string(),
        byte_size: 4,
        sha256: "sha".to_string(),
        storage_key: "server/key".to_string(),
        created_at: chrono::Utc::now(),
        deleted_at: None,
    };

    core.save_remote_asset(&asset, &[1, 2, 3, 4]).unwrap();

    let asset_path = dir
        .path()
        .join(format!("assets/notes/{note_id}/{}.png", asset.id));
    assert_eq!(std::fs::read(asset_path).unwrap(), vec![1, 2, 3, 4]);
}

#[test]
fn bootstrap_shows_local_notes_when_logged_out_and_account_notes_when_logged_in() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    assert_eq!(core.bootstrap().unwrap().notes.len(), 1);

    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    assert!(core.bootstrap().unwrap().notes.is_empty());

    core.import_anonymous_notes_to_current_account().unwrap();
    assert_eq!(core.bootstrap().unwrap().notes.len(), 1);
}

#[test]
fn importing_anonymous_notes_enqueues_upserts_for_current_account() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    core.import_anonymous_notes_to_current_account().unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert!(changes
        .iter()
        .all(|item| item.account_id.as_deref() == Some("acct_a")));
    assert!(changes.iter().any(|item| item.note_id == local.id));
}

#[test]
fn importing_anonymous_notes_removes_old_anonymous_queue_rows() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    let payload = snapline_domain::SyncPayload::Note(
        snapline_domain::NoteChangePayload::from_note(&local),
    );
    core.repo
        .enqueue_change(
            None,
            &local.id,
            snapline_domain::SyncOpType::UpsertNote,
            0,
            &payload,
            chrono::Utc::now(),
        )
        .unwrap();

    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();
    core.import_anonymous_notes_to_current_account().unwrap();

    assert!(core.repo.list_pending_changes(None, 10).unwrap().is_empty());
    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
}

#[test]
fn logged_in_account_cannot_modify_anonymous_note_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
        .unwrap();

    assert!(core
        .save_note(&local.id, "Account edit", "Account edit", false)
        .is_err());
    assert!(core.set_note_pinned(&local.id, true).is_err());
    assert!(core.delete_note(&local.id).is_err());
    assert!(core.save_png_asset(&local.id, &[137, 80, 78, 71]).is_err());
}
