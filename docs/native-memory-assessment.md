# Native memory assessment

Observed: 2026-10-26. Target: OpenClaw **v2026.10.7**. Scope: T008 explicit personal-memory lifecycle. This is source/documentation evidence, not installed SDK or runtime acceptance. No native memory, live configuration, provider or database was changed.

## Verified native facilities

The pinned Memory Core entry registers `memory_search`, `memory_get` and `intent`; its manifest declares those same tools. The entry describes a file-backed memory integration and supplies its existing search manager and memory flush resolver. It does not register secretary-style save/update/archive/history tools. This finding describes the inspected provider, rather than proving all possible providers lack those capabilities. [Pinned Memory Core entry](https://github.com/openclaw/openclaw/blob/v2026.10.7/extensions/memory-core/index.ts), [pinned manifest](https://github.com/openclaw/openclaw/blob/v2026.10.7/extensions/memory-core/openclaw.plugin.json).

Native Gateway read methods are documented in the pinned release: `memory.search` with `version: 2` returns provider-scoped references, `memory.get` resolves such a reference, and `memory.status` reports selected-provider health. They require authenticated operator read authority. The pinned handler registry assigns the family to `server-methods/memory-search.ts`. That file and its exact request/result schemas did not load through the available web fetcher in this assessment, so method existence is verified but guessed request keys are not justified. [Pinned RPC documentation](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/gateway/protocol/rpc-system-and-channels.md), [pinned handler registry](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/gateway/server-methods/core-handlers.ts).

The current SDK documentation describes neutral search/get/health with provider-scoped references and caller-bound authority. Rich mutation remains with provider tools. It also describes a private-local bundled-consumer facade; its presence is not permission for a companion plugin to import provider internals. Current docs cannot replace verification against the pinned exports. [Current memory SDK documentation](https://docs.openclaw.ai/plugins/sdk-overview/memory-and-context).

## Explicit saving and automatic persistence

Pinned memory guidance uses `USER.md` for preferences, `MEMORY.md` for durable facts, and dated memory notes for working context. It encourages ordinary agent writes as work proceeds. Pre-compaction flush and dreaming are automatic persistence paths, with dreaming enabled by default. Disabling the compaction flush therefore does not establish explicit-only saving. [Pinned memory overview](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/concepts/memory.md).

The pinned flush resolver returns no plan when `agents.defaults.compaction.memoryFlush.enabled` is `false`; otherwise it targets a dated memory Markdown file. This is a verified supported control for that one persistence path. [Pinned flush resolver](https://github.com/openclaw/openclaw/blob/v2026.10.7/extensions/memory-core/src/flush-plan.ts).

The pinned configuration reference additionally documents:

- `plugins.entries.memory-core.config.dreaming.enabled: false` to disable dreaming.
- `memory.search.rememberAcrossConversations: false` to avoid its automatic addition of session transcripts; restricting `sources` alone is insufficient.
- The separate session-memory hook can save excerpts into the memory source.
- Session admission exclusions do not block direct agent writes or session-memory hooks; tool permissions and hook configuration are separate controls.

These settings do not erase existing memories or disable ordinary session transcript storage. A complete explicit-only qualification must inspect every enabled persistence path and general file/shell capabilities. [Pinned memory configuration](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/reference/memory-config.md).

## Agent files and provider choice

Current RPC documentation gives `agents.files.get`/`set` content hashes, conditional `expectedHash` or `expectedMissing` writes, and an optional `expectedWorkspace` binding. This is bootstrap-file editing, not an explicit memory record lifecycle. It does not establish memory archive exclusion or a preference revision-history API. The corresponding pinned `agents-files.ts` schema and `agents.ts` handler did not load in this assessment. Do not implement broad file replacement or invent an embedded record journal as a substitute for qualified memory semantics. [Current agent-file RPC documentation](https://docs.openclaw.ai/gateway/protocol/rpc-talk-config-and-agents).

Pinned user-model guidance supports active/superseded directives, but says personal-instructions tooling is unavailable on a single-user Gateway; shared workspace `USER.md` remains the single-user facility. Its documented file conflict checking against independent host editors is best effort. This does not satisfy durable secretary record history by itself. [Pinned user-model documentation](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/concepts/user-model.md).

The current Knowledge plugin documentation may be useful for later provider evaluation, but its pinned manifest/guide could not be retrieved here. Selecting a different memory provider would be an explicit architecture/configuration choice, not an assumed existing capability. No recommendation here authorizes enabling it. [Current Knowledge documentation](https://docs.openclaw.ai/plugins/knowledge).

## Recommended implementation boundary

The supported direction is a **partial native read adapter** through Gateway memory search/get/status after verifying exact pinned payloads and admitted-run authority. Bind the configured agent and owner outside model arguments; preserve native references and provenance, bound returned data, recheck invocation authority after asynchronous reads, and never convert provider denial or unavailability to an empty successful search. Keep it independently disabled until actual host checks pass. Do not register native Memory Core tool names as secretary-owned tools.

Keep save/update/archive/history explicitly unavailable until an inspected provider exposes suitable lifecycle and authorization semantics. A local capability response can distinguish supported reads from unavailable mutations, but does not make T008 complete. A native memory note that remains indexed after being marked archived cannot be presented as a successful archive.

Before qualifying explicit-only saving, inspect the effective agent configuration, disable the documented automatic paths, audit hooks and broad write/shell tools, then prove on disposable native resources that casual conversation/compaction/reset/background turns do not create personal memory. Verify deliberate saving, correction, archive exclusion and revision history separately when a provider is selected. Production enablement remains T007.
