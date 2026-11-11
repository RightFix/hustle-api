CREATE TABLE admin_audit_logs (
    id SERIAL PRIMARY KEY,
    actor_id INT NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id INT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX admin_audit_logs_created_idx ON admin_audit_logs (created_at DESC);
CREATE INDEX admin_audit_logs_actor_idx ON admin_audit_logs (actor_id, created_at DESC);
CREATE INDEX admin_audit_logs_target_idx ON admin_audit_logs (target_type, target_id);
