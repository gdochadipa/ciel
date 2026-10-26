# Secretary security boundaries

The operator-controlled registry and Rust Core enforce access. Skills, model arguments, spreadsheet notes and remembered text grant no authority. Existing workbook tabs start read-only; structured writes require stable identity and a reviewed mapping.

Business HTTP calls require the Core bearer token bound to the sole owner. Approval uses a distinct operator token and immutable preview digest through an operator-only endpoint. The approval token is never a model tool input or response. Keep writes disabled until the trusted OpenClaw hook, owner identity, complete diff presentation, expiry, replay protection and restart behavior are qualified. A callback decision alone is not a signed approval proof. Every mutation is recorded before provider execution; uncertain provider results require reconciliation and cannot be blindly replayed.

Use a dedicated WhatsApp account, explicit owner DM admission and disabled inbound groups. Inbound `allowFrom` does not restrict outbound recipients. The bridge must enforce its own destination allowlist independently; examples in this repository are configuration templates, not active channel policy. Disable self-originated DMs for the dedicated assistant account. Skills cannot send vendor messages or change destinations.

Store runtime environment and service-account files outside the repository with owner-only permissions. Use distinct random Core and approval tokens; never print them during checks, log raw provider errors, place credentials in prompts, or copy them into fixtures. Bind the service to loopback and run it as a dedicated non-root account. Localhost still requires authentication.

Native memory currently has no qualified save/update/archive/history adapter here. Keep memory mutations unavailable and separately disable automatic flush/dreaming/capture before claiming explicit-save-only behavior. Source financial facts come from Sheets rather than conversation memory. Retain only necessary private data and define operational retention before activation.

Production activation requires T007's actual runtime, Google, approval, scheduling and WhatsApp evidence. Local builds and HTTP stand-ins establish local behavior only. The subsequent low-effort code review and regression fixes do not establish a security audit or production certification.
