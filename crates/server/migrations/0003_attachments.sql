CREATE TABLE attachments (
    id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id UUID NOT NULL,
    total_size BIGINT NOT NULL CHECK (total_size > 0),
    part_size BIGINT NOT NULL CHECK (part_size > 0),
    total_parts INTEGER NOT NULL CHECK (total_parts > 0),
    ciphertext_sha256 TEXT NOT NULL,
    encrypted_metadata TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'uploading' CHECK (status IN ('uploading', 'complete')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, id)
);

CREATE TABLE attachment_parts (
    user_id UUID NOT NULL,
    attachment_id UUID NOT NULL,
    part_number INTEGER NOT NULL CHECK (part_number >= 0),
    size BIGINT NOT NULL CHECK (size > 0),
    ciphertext_sha256 TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, attachment_id, part_number),
    FOREIGN KEY (user_id, attachment_id) REFERENCES attachments(user_id, id) ON DELETE CASCADE
);
