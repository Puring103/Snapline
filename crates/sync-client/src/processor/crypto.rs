use anyhow::Result;
use chrono::Utc;
use snapline_domain::{
    crypto::{decrypt_field, encrypt_field},
    Note, NoteChangePayload, SyncPayload,
};

pub(super) fn local_note_from_pending(
    pending: &[snapline_storage::ChangeQueueItem],
    queue_id: &str,
    note_id: &snapline_domain::NoteId,
) -> Option<Note> {
    let item = pending.iter().find(|item| item.id == queue_id)?;
    let SyncPayload::Note(payload) = &item.payload else {
        return None;
    };
    let now = Utc::now();
    Some(Note {
        id: note_id.clone(),
        title: payload.title.clone(),
        content_md: payload.content_md.clone(),
        pinned: payload.pinned,
        created_at: now,
        updated_at: now,
        deleted_at: payload.deleted_at,
        server_version: item.base_version,
        last_modified_by_device: None,
        is_conflict_copy: false,
        source_note_id: None,
        owner_account_id: item.account_id.clone(),
    })
}

pub(super) fn encrypt_note_payload(
    dek: &[u8; 32],
    payload: &NoteChangePayload,
) -> Result<NoteChangePayload> {
    Ok(NoteChangePayload {
        title: encrypt_field(dek, &payload.title)?,
        content_md: encrypt_field(dek, &payload.content_md)?,
        pinned: payload.pinned,
        deleted_at: payload.deleted_at,
    })
}

pub(super) fn decrypt_note(dek: &[u8; 32], note: &Note) -> Result<Note> {
    Ok(Note {
        title: decrypt_field(dek, &note.title)?,
        content_md: decrypt_field(dek, &note.content_md)?,
        ..note.clone()
    })
}
