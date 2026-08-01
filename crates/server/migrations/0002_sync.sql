CREATE TABLE sync_objects (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    object_id UUID NOT NULL,
    object_type TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    ciphertext TEXT NOT NULL,
    nonce TEXT NOT NULL,
    wrapped_key TEXT NOT NULL,
    device_id UUID NOT NULL,
    client_updated_at TIMESTAMPTZ NOT NULL,
    server_updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, object_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, id)
);

CREATE TABLE sync_changes (
    cursor BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    object_id UUID NOT NULL,
    object_type TEXT NOT NULL,
    version BIGINT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    ciphertext TEXT NOT NULL,
    nonce TEXT NOT NULL,
    wrapped_key TEXT NOT NULL,
    device_id UUID NOT NULL,
    client_updated_at TIMESTAMPTZ NOT NULL,
    server_created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, id)
);

CREATE INDEX sync_changes_user_cursor_idx ON sync_changes(user_id, cursor);

CREATE TABLE sync_push_requests (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    idempotency_key UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, idempotency_key)
);

CREATE TABLE sync_acks (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id UUID NOT NULL,
    cursor BIGINT NOT NULL CHECK (cursor >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, device_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, id) ON DELETE CASCADE
);

