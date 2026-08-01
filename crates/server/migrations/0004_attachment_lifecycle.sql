ALTER TABLE attachments
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX attachments_stale_upload_idx
    ON attachments(updated_at)
    WHERE status = 'uploading';
