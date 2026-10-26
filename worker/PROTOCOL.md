# Worker protocol v1

Launch the packaged interpreter with `-I -u` and the absolute path to `main.py`.
One process handles one request and exits. stdin accepts one UTF-8 JSON line (at
most 1 MiB): `protocol`, `job_id`, `attempt_id`, `command`, `payload`. IDs are
1–80 ASCII letters, numbers, hyphens or underscores. Commands are `healthcheck`,
`inspect`, `enumerate` and `download`; adapters are implemented incrementally.

stdout contains JSON Lines (at most 2 MiB each). Every envelope has `protocol`,
`job_id`, `attempt_id`, `event` and `data`. A valid request produces `hello`,
zero or more intermediate events, then exactly one `result` or `error`. An
invalid request produces an error with null IDs. Success exits 0; failure exits
1. A process exit without a terminal event is a supervisor-level failure.

Error data has `code`, `message`, `retryable`. Codes distinguish `protocol`,
`runtime`, `engine`, `network`, `source`, `destination`, `format`, `cancelled`.
The supervisor must reject mismatched IDs, oversized lines and duplicate
terminal messages, and ignore stale attempts. Never expose raw signed media
URLs or credentials in logs. stdout is exclusively the protocol stream.
