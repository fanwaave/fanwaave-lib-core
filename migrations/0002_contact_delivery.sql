-- fanwaave-0002: canonical outbound contact-delivery schema.
-- This file is authoritative. Consumers (API/worker/ORM) must not maintain divergent DDL.

CREATE TABLE IF NOT EXISTS contact_campaigns (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL,
    channel text NOT NULL CHECK (channel IN ('email', 'sms')),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'queued', 'running', 'completed', 'cancelled')),
    total_count bigint NOT NULL DEFAULT 0 CHECK (total_count >= 0),
    attempted_count bigint NOT NULL DEFAULT 0 CHECK (attempted_count >= 0),
    sent_count bigint NOT NULL DEFAULT 0 CHECK (sent_count >= 0),
    failed_count bigint NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    suppressed_count bigint NOT NULL DEFAULT 0 CHECK (suppressed_count >= 0),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    completed_at timestamptz
);

CREATE TABLE IF NOT EXISTS contact_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key text NOT NULL,
    campaign_id uuid REFERENCES contact_campaigns(id) ON DELETE SET NULL,
    channel text NOT NULL CHECK (channel IN ('email', 'sms')),
    provider text NOT NULL CHECK (provider IN ('sendgrid', 'twilio')),
    payload jsonb NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'leased', 'retry', 'sent', 'dead', 'cancelled')),
    priority integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    lease_owner text,
    lease_expires_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts integer NOT NULL DEFAULT 8 CHECK (max_attempts > 0),
    last_error text,
    provider_message_id text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    sent_at timestamptz,
    CONSTRAINT contact_jobs_idempotency_key_unique UNIQUE (idempotency_key),
    CONSTRAINT contact_jobs_payload_object CHECK (jsonb_typeof(payload) = 'object')
);

CREATE INDEX IF NOT EXISTS contact_jobs_claim_idx
    ON contact_jobs (priority DESC, available_at ASC, created_at ASC)
    WHERE status IN ('queued', 'retry', 'leased');
CREATE INDEX IF NOT EXISTS contact_jobs_campaign_idx ON contact_jobs (campaign_id, status);
CREATE INDEX IF NOT EXISTS contact_jobs_lease_idx ON contact_jobs (lease_expires_at) WHERE status = 'leased';

CREATE TABLE IF NOT EXISTS contact_attempts (
    id bigserial PRIMARY KEY,
    job_id uuid NOT NULL REFERENCES contact_jobs(id) ON DELETE CASCADE,
    attempt_no integer NOT NULL CHECK (attempt_no > 0),
    worker_id text NOT NULL,
    provider text NOT NULL,
    outcome text NOT NULL CHECK (outcome IN ('sent', 'retryable', 'permanent_failure', 'suppressed')),
    upstream_status integer,
    provider_message_id text,
    error text,
    rate_limited boolean NOT NULL DEFAULT false,
    latency_ms bigint NOT NULL DEFAULT 0 CHECK (latency_ms >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contact_attempts_job_attempt_unique UNIQUE (job_id, attempt_no)
);
CREATE INDEX IF NOT EXISTS contact_attempts_job_idx ON contact_attempts (job_id, created_at DESC);

CREATE TABLE IF NOT EXISTS contact_suppressions (
    id bigserial PRIMARY KEY,
    channel text NOT NULL CHECK (channel IN ('email', 'sms')),
    destination text NOT NULL,
    reason text NOT NULL CHECK (reason IN ('unsubscribe', 'bounce', 'complaint', 'invalid', 'manual')),
    source text NOT NULL DEFAULT 'fanwaave',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contact_suppressions_destination_unique UNIQUE (channel, destination)
);

CREATE TABLE IF NOT EXISTS contact_delivery_events (
    id bigserial PRIMARY KEY,
    provider text NOT NULL,
    provider_event_id text,
    provider_message_id text,
    job_id uuid REFERENCES contact_jobs(id) ON DELETE SET NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    occurred_at timestamptz,
    received_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS contact_delivery_events_provider_event_unique
    ON contact_delivery_events (provider, provider_event_id)
    WHERE provider_event_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS contact_delivery_events_message_idx
    ON contact_delivery_events (provider, provider_message_id)
    WHERE provider_message_id IS NOT NULL;
