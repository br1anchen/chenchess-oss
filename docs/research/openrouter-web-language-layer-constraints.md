# OpenRouter constraints for the web Language Layer

Research date: 2026-08-08 (every OpenRouter fact below was fetched on this date; the docs and the
live model catalogue change frequently, so treat prices, slugs, and endpoint capability flags as
**snapshot facts that must be re-verified at pin time**, not as durable API contract). Vertex AI
429, backoff, and timeout facts in area 11 were fetched from first-party Google Cloud docs on
2026-08-21.

Research asset for [Ship the tailored OpenRouter web Language Layer to beta](#229).
Companion to [Seam inventory: OpenRouter-backed web Language Layer](./openrouter-web-language-layer-seams.md),
which covers what the repository already provides.

## Question

What do current primary OpenRouter and routed-provider docs establish about API compatibility,
structured output, exact model pinning, prompt retention, data residency, rate limits, cost
reporting, cancellation, outage behaviour, and model versioning — and which small set of pinned
model slugs is credible for grounded Review Session authoring?

## Sources and method

Only first-party sources were used:

- OpenRouter documentation under `https://openrouter.ai/docs/...`, the OpenRouter privacy policy,
  and the machine-readable OpenAPI specification at `https://openrouter.ai/openapi.json`.
- The **live OpenRouter API** (`/api/v1/models`, `/api/v1/models/{slug}/endpoints`,
  `/api/v1/providers`, `/api/v1/endpoints/zdr`), queried unauthenticated on 2026-08-08. This is the
  strongest primary source for per-model and per-endpoint facts, because the marketing model pages
  render from the same data.
- First-party provider docs: Anthropic commercial terms, OpenAI API data-usage guide.

Where the OpenAPI spec and the prose docs disagree, the **spec is treated as authoritative** and the
disagreement is called out. The docs site was mid-reorganisation on the fetch date: several
documented paths (`/docs/features/...`) 404 or redirect, and the same content is reachable under
`/docs/guides/...` and `/docs/api_reference/...`. URLs below are the ones that actually resolved.

## Answer

Nine load-bearing conclusions, in the order the design will hit them.

1. **The surface is OpenAI Chat Completions plus OpenRouter extensions, and the dangerous default is
   silent parameter dropping.** Unsupported parameters are not rejected — they are stripped per
   provider. A `response_format` request can be routed to an endpoint that ignores it and returns
   prose. `provider.require_parameters: true` is the only documented defence and it is **off by
   default**.
2. **Structured output is available on every candidate model, but not on every endpoint of those
   models.** Verified live: `anthropic/claude-haiku-4.5` supports `structured_outputs` on the
   Anthropic, Azure, and Bedrock endpoints, and **does not** on any of the three `google-vertex/*`
   endpoints. Provider choice, not model choice, decides whether the Grounding Gate gets JSON.
3. **`strict: true` is a request, not a guarantee.** OpenRouter's own words: "exact compliance is not
   guaranteed on every endpoint". The existing retry-once-then-safe-render Grounding Gate is the
   correct shape and must stay.
4. **Exact pinning is achievable and requires three things together**: a dated permaslug as `model`,
   `provider.only` (or `order`) plus `provider.allow_fallbacks: false`, and **no** `models` array.
   Any one of these omitted re-opens substitution.
5. **Substitution is auditable after the fact.** Every response carries `openrouter_metadata` with
   `requested`, `strategy`, `attempt`, and the selected endpoint, and `usage` carries the real
   `cost`. That is enough to populate a real Evaluation Fingerprint and to alarm on any request that
   did not run on the pinned endpoint.
6. **"Prompts are never trained on" is reachable but only as a conjunction**: account-level privacy
   setting, plus `provider.data_collection: "deny"`, plus `provider.zdr: true`, plus pinning to a
   provider whose own first-party terms forbid training. OpenRouter itself states it does not train
   on inputs or outputs; it explicitly pushes provider-training responsibility onto the caller.
7. **ZDR and structured output can conflict.** For `anthropic/claude-haiku-4.5` the direct
   `anthropic` endpoint is **not** in the live ZDR list, while `amazon-bedrock/*` and
   `google-vertex/*` are — and `google-vertex/*` lacks structured outputs. ZDR **and** structured
   output for Haiku 4.5 today means Bedrock.
8. **Cost is exact and per-request, but cancellation billing is provider-dependent.** Aborting a
   stream stops billing only "for supported providers"; OpenAI, Anthropic-hosted, and Azure paths
   generally support it, Bedrock and Google do not. A cancelled Review Session turn on a Bedrock pin
   is billed in full.
9. **Rate limits for paid models are not documented as numbers.** Only free-variant limits are
   published. Capacity planning for centrally-funded beta traffic cannot be derived from the docs.
   That absence is a finding, not a research gap: #331 does not recover a number by saturating the
   pinned route. Vertex publishes 429 semantics on the routed provider (area 11) but its quota
   figures belong to OpenRouter's GCP project, never to ChenChess traffic. Coach Engine honours
   the provider's `Retry-After` (or the 1 s floor) and reads observed latency and 429s from
   Language Layer Operational Records once staging carries hosted traffic.

Recommended shortlist for Review Session authoring, in preference order:
`anthropic/claude-haiku-4.5` (pinned to `amazon-bedrock`), `google/gemini-3.5-flash-lite`,
`anthropic/claude-sonnet-5`, `openai/gpt-5.6-luna`. Details and prices in area 10.

---

## 1. API compatibility and structured output

### 1.1 The Chat Completions surface

The endpoint is `POST https://openrouter.ai/api/v1/chat/completions`, described as a schema "very
similar to the OpenAI Chat API, with a few small differences", with OpenRouter normalising the schema
across models and providers
([API reference overview](https://openrouter.ai/docs/api-reference/overview), fetched 2026-08-08).

Authentication is `Authorization: Bearer <OPENROUTER_API_KEY>`; `HTTP-Referer` and
`X-OpenRouter-Title` are optional app-attribution headers (same page).

From the OpenAPI spec (`https://openrouter.ai/openapi.json`, `components.schemas.ChatRequest`,
fetched 2026-08-08), the only required field is `messages`. Fields that matter to this design:

| Field | Type | Notes from the spec |
| --- | --- | --- |
| `model` | string | `ModelName`, "Model to use for completion" |
| `models` | string[] | `ChatModelNames`, fallback list — see area 2 |
| `provider` | object | `ProviderPreferences` — see area 2 |
| `response_format` | oneOf | discriminated on `type`: `text`, `json_object`, `json_schema`, `grammar`, `python` |
| `stream` | boolean | default `false` |
| `stream_options` | object | only property is `include_usage`, marked **deprecated**, "This field has no effect. Full usage details are always included." |
| `route` | enum | **`DeprecatedRoute`** — see area 2 |
| `max_tokens` | int | marked deprecated in favour of `max_completion_tokens`; "some providers enforce a minimum of 16" |
| `seed` | int | "Random seed for deterministic outputs" — provider support varies |
| `user` | string | "Per-end-user identifier for abuse isolation. Use a stable ID, hash, or pseudonym." |
| `metadata` | object | max 16 pairs, 64-char keys, 512-char values |
| `session_id` | string | sticky routing key for grouping related requests |
| `service_tier` | string | `fast` is an alias for `priority` |
| `debug.echo_upstream_body` | bool | returns the exact upstream body; streaming-only, "should never be used in production" ([errors doc](https://openrouter.ai/docs/api-reference/errors)) |

There is **no `usage` request field** in `ChatRequest`. This contradicts the widely-copied
`usage: {include: true}` idiom — see area 6.

Response shape (`ChatResult` in the spec): `id`, `model`, `object`, `created`, `choices`,
`usage`, `service_tier`, `system_fingerprint`, `openrouter_metadata`.

### 1.2 The load-bearing difference from OpenAI

> "Parameters unsupported by specific models are silently ignored rather than causing errors."
> — [API reference overview](https://openrouter.ai/docs/api-reference/overview)

The spec says the same thing from the routing side, on `provider.require_parameters`:

> "Whether to filter providers to only those that support the parameters you've provided. If this
> setting is omitted or set to false, then providers will receive only the parameters they support,
> and ignore the rest."
> — `components.schemas.ProviderPreferences.properties.require_parameters`, `openapi.json`

**Design consequence.** A Review Session request that asks for `response_format: json_schema` can be
served by an endpoint that drops it, returning ordinary prose with HTTP 200. The Grounding Gate would
see a parse failure rather than a routing failure. `require_parameters: true` must be set on every
authoring call, and the endpoint capability must additionally be pinned (area 2), because
`require_parameters` narrows the candidate set but does not by itself name one.

### 1.3 Structured outputs

[Structured outputs](https://openrouter.ai/docs/features/structured-outputs) (fetched 2026-08-08)
documents `response_format: { type: "json_schema", json_schema: { name, schema, strict, description } }`.
The spec's `ChatJsonSchemaConfig` confirms: `name` is required (max 64 chars,
`a-z A-Z 0-9 _ -`), `schema` and `strict` and `description` are optional, and `strict` is described
as "Enable strict schema adherence".

Guarantees, in OpenRouter's own words:

- Use `strict: true` to enforce schema compliance, **"though exact compliance is not guaranteed on
  every endpoint"**.
- "the same model may be served by multiple providers, and only some of those providers may support
  structured outputs."
- Recommended discovery path: filter the models page for `structured_outputs`, set
  `require_parameters: true`, and include `response_format`.
- Streaming: "the model will stream valid partial JSON that, when complete, forms a valid response
  matching your schema".
- A **Response Healing plugin** is offered for non-streaming requests to mitigate formatting issues.

Tool calling is available through the standard `tools` / `tool_choice` / `parallel_tool_calls`
fields (spec, `ChatRequest`).

Per-model support is exposed programmatically as `structured_outputs` inside `supported_parameters`
on `/api/v1/models` and, crucially, **per endpoint** on `/api/v1/models/{slug}/endpoints`.

**Verified live on 2026-08-08** (`GET https://openrouter.ai/api/v1/models/anthropic/claude-haiku-4.5/endpoints`):

| Endpoint tag | `structured_outputs` | `response_format` |
| --- | --- | --- |
| `anthropic` | yes | yes |
| `azure/global` | yes | yes |
| `amazon-bedrock/global` | yes | yes |
| `amazon-bedrock/eu-west-1` | yes | yes |
| `google-vertex/global` | **no** | **no** |
| `google-vertex/europe` | **no** | **no** |
| `google-vertex/us-east5` | **no** | **no** |

The same asymmetry appears on `anthropic/claude-sonnet-5`, where the three `google-vertex/*`
endpoints lack `structured_outputs` while `anthropic`, `azure/global`, and all `amazon-bedrock/*`
endpoints have it. This is the single most actionable finding in the document.

### 1.4 SDK options

- OpenAI SDKs work by pointing `base_url` at `https://openrouter.ai/api/v1`. OpenRouter-specific
  fields that the OpenAI SDK does not model (notably `models`) must go in `extra_body`
  ([model fallbacks](https://openrouter.ai/docs/guides/routing/model-fallbacks)).
- An Anthropic-shaped `/api/v1/messages` endpoint exists (OpenAPI paths), where the fallback
  parameter is named `fallbacks`, accepts up to 3 entries with only a `model` field, and **cannot be
  combined with `models`** — doing so returns 400 (same page).
- First-party OpenRouter TypeScript and Python SDKs are documented under `/docs/client-sdks/...`.
- A `/api/v1/responses` endpoint (OpenAI Responses shape) also exists in the spec.

For a Rust caller with no first-party SDK, the raw HTTP surface plus the OpenAPI spec is the
contract. Nothing in the docs requires an SDK.

---

## 2. Exact model pinning and disabling substitution

### 2.1 Slugs, permaslugs, and the `~` alias family

`/api/v1/models` returns both `id` (the slug you send) and `canonical_slug`, documented as a
"Permanent slug for the model that never changes"
([models docs](https://openrouter.ai/docs/models)). In practice the canonical slug is the dated
revision. Live examples, 2026-08-08:

| `id` | `canonical_slug` |
| --- | --- |
| `anthropic/claude-haiku-4.5` | `anthropic/claude-4.5-haiku-20251001` |
| `anthropic/claude-sonnet-5` | `anthropic/claude-sonnet-5-20260630` |
| `anthropic/claude-opus-5` | `anthropic/claude-opus-5-20260723` |
| `anthropic/claude-fable-5` | `anthropic/claude-5-fable-20260609` |
| `google/gemini-3.5-flash-lite` | `google/gemini-3.5-flash-lite-20260721` |
| `openai/gpt-5.6-luna` | `openai/gpt-5.6-luna-20260709` |

Note the **inconsistent ordering** of the version token inside canonical slugs
(`claude-4.5-haiku-…` vs `claude-sonnet-5-…` vs `claude-5-fable-…`). Do not parse these; store them
verbatim.

The catalogue also contains a family of slugs prefixed with `~`, which are floating "latest"
aliases: `~anthropic/claude-haiku-latest`, `~anthropic/claude-sonnet-latest`,
`~anthropic/claude-opus-latest`, `~anthropic/claude-fable-latest`, `~google/gemini-flash-latest`,
`~google/gemini-pro-latest`, `~openai/gpt-latest`, `~openai/gpt-mini-latest`. Their `canonical_slug`
equals their `id` — i.e. **they carry no dated revision at all**. These are precisely what a
reproducible Evaluation Fingerprint must never use. *Not verified from a primary source: whether the
`~` prefix has documented semantics anywhere in the docs; I could not find a page defining it. It is
inferred from the live catalogue.*

Aliases are resolved server-side: "the API resolves aliases automatically", e.g.
`anthropic/claude-3-5-sonnet` → `anthropic/claude-3.5-sonnet`
([models docs](https://openrouter.ai/docs/models)).

Variant suffixes appended with `:` are documented as `:free`, `:thinking`, `:nitro`, `:floor`,
`:online`, `:extended` (same page); `:batch` appears live in the catalogue at roughly half price
(e.g. `anthropic/claude-opus-5:batch` at $2.50/$12.50 vs $5/$25). Per
[provider routing](https://openrouter.ai/docs/guides/routing/provider-selection), `:nitro` sorts by
throughput and `:floor` sorts by price — i.e. **both are substitution-widening**, the opposite of
pinning. `:online` attaches web search. **None of `:nitro`, `:floor`, `:online` belong on a grounded
authoring call.**

### 2.2 Can OpenRouter silently substitute? Yes, in three independent ways

1. **Provider substitution within a model.** `provider.allow_fallbacks` defaults to `true`:
   > "true: (default) when the primary provider (or your custom providers in "order") is
   > unavailable, use the next best provider. false: use only the primary/custom provider, and
   > return the upstream error if it's unavailable."
   > — `ProviderPreferences.allow_fallbacks`, `openapi.json`

   And the FAQ: "If a provider returns an error OpenRouter will automatically fall back to the next
   provider" ([FAQ](https://openrouter.ai/docs/faq)).

   Default behaviour with no `provider` block at all is load balancing: requests "load balance across
   providers, prioritizing price"
   ([provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)).

2. **Model substitution via `models`.**
   > "Provide an array of model IDs in priority order. If the first model returns an error,
   > OpenRouter will automatically try the next model in the list."
   > — [model fallbacks](https://openrouter.ai/docs/guides/routing/model-fallbacks)

   Triggers listed: context-length validation errors, moderation flags, rate limiting, provider
   downtime. Billing follows the model actually used, "which will be returned in the `model`
   attribute of the response body". This only happens if you opt in by sending `models`.

3. **Auto Router**, if `model` is `openrouter/auto` or `openrouter/auto-beta`
   ([model routing](https://openrouter.ai/docs/features/model-routing)). Never use it here.

Note that substitution is *not* silent in the strict sense — the response's `model` field and
`openrouter_metadata` report what ran — but it is silent to a caller that does not check.

### 2.3 The pinning recipe

```json
{
  "model": "anthropic/claude-haiku-4.5",
  "provider": {
    "only": ["amazon-bedrock"],
    "allow_fallbacks": false,
    "require_parameters": true,
    "data_collection": "deny",
    "zdr": true
  },
  "response_format": { "type": "json_schema", "json_schema": { "name": "...", "strict": true, "schema": {} } }
}
```

with **no** `models` array, **no** `route`, and **no** `:nitro` / `:floor` / `:online` suffix.

Field semantics, all quoted from `openapi.json` (`ProviderPreferences`) on 2026-08-08:

| Field | Default | Exact description |
| --- | --- | --- |
| `allow_fallbacks` | `true` | as quoted above |
| `only` | – | "List of provider slugs to allow. If provided, this list is merged with your account-wide allowed provider settings for this request." |
| `ignore` | – | "List of provider slugs to ignore. If provided, this list is merged with your account-wide ignored provider settings for this request." |
| `order` | – | "An ordered list of provider slugs. The router will attempt to use the first provider in the subset of this list that supports your requested model, and fall back to the next if it is unavailable. If no providers are available, the request will fail with an error message." |
| `require_parameters` | `false` | as quoted in 1.2 |
| `data_collection` | `"allow"` | "allow: (default) allow providers which store user data non-transiently and may train on it. deny: use only providers which do not collect user data." Plus: "If no available model provider meets the requirement, your request will return an error." |
| `zdr` | – | "Whether to restrict routing to only ZDR (Zero Data Retention) endpoints. When true, only endpoints that do not retain prompts will be used." |
| `quantizations` | – | "A list of quantization levels to filter the provider by." (`int4`, `int8`, `fp8`, `bf16`, …) |
| `sort` | – | `{ by: price\|throughput\|latency\|exacto, partition: model\|none }`; "The sorting strategy to use for this request, if "order" is not specified. When set, no load balancing is performed." |
| `max_price` | – | per-million-token caps for `prompt` / `completion`, plus `image`, `audio`, `request` |
| `enforce_distillable_text` | – | restrict to models whose author allows text distillation |
| `preferred_min_throughput`, `preferred_max_latency` | – | soft de-prioritisation, not hard filters |

Two important notes.

- **`only` and `ignore` merge with account-wide settings.** Request-level pinning does not fully
  describe the effective routing set; the account configuration is part of the pin. Any Evaluation
  Fingerprint that claims reproducibility across environments must therefore also record the account
  configuration, or the deployment must guarantee it.
- **`route` is deprecated.** The spec marks `DeprecatedRoute` with:
  > "**DEPRECATED** Use providers.sort.partition instead. Backwards-compatible alias for
  > providers.sort.partition. Accepts legacy values: "fallback" (maps to "model"), "sort" (maps to
  > "none")."

  So `route: "fallback"` should not be written in new code; it is now a `sort.partition` alias, and
  in any case `sort` is only consulted when `order` is absent.

Provider slugs come from `GET /api/v1/providers` (101 entries on 2026-08-08) or the copy button on
model pages. **Endpoint tags are finer than provider slugs** — e.g. `amazon-bedrock/global`,
`amazon-bedrock/eu-west-1`, `google-vertex/europe`, `openai/flex`, `openai/priority`. Whether
`provider.only` accepts a full endpoint tag (`amazon-bedrock/eu-west-1`) or only the provider slug
(`amazon-bedrock`) is *not clearly documented*; the provider-routing page shows a variant-bearing
example (`"order": ["deepinfra/turbo"]`) and says the copy button yields "the exact provider slug,
including any variants like "/turbo"", which implies tags are accepted in `order`. **Verify
empirically before relying on region-level pinning.**

### 2.4 Verifying the pin held

`ChatResult.openrouter_metadata` (`OpenRouterMetadata` in the spec) is required on responses and
carries:

- `requested` — the model slug you asked for
- `strategy` — routing strategy (e.g. `"direct"`)
- `attempt` (int) and `attempts[]` (`RouterAttempt`)
- `endpoints` — `{ total, available: [{ model, provider, selected }] }`
- `region` (e.g. `"iad"`), `is_byok`, `summary` (e.g. `"available=1, selected=OpenAI"`), `pipeline[]`

Plus the top-level `model` field and, per the streaming docs, the `X-Generation-Id` response header,
which "is returned in the `X-Generation-Id` response header for all endpoints"
([streaming](https://openrouter.ai/docs/api-reference/streaming)).

**Design consequence.** The Language Layer adapter should assert
`openrouter_metadata.endpoints.available[selected].provider` and the response `model` against the pin
on every call, and fail the Grounding Gate closed on mismatch rather than trusting the request
parameters. This turns "pinned" from a hope into an invariant, and it is what makes the existing
`CriticalMomentExplainerCandidate` provenance honest.

---

## 3. Prompt retention and training controls

### 3.1 OpenRouter's own position

From the [OpenRouter privacy policy](https://openrouter.ai/privacy) (fetched 2026-08-08):

- "OpenRouter does not use your Inputs or Outputs for model training."
- "Some Model Providers may use your Inputs and Outputs for model training or improvement."
- "If you do not want your Inputs used for model training, select a Model or Model Provider that
  commits to not using your data for that purpose."
- "We do not persist image, audio or video files beyond the duration necessary to route the request,
  except as required for abuse detection, security, billing, or legal compliance."

From the [FAQ](https://openrouter.ai/docs/faq):

- "We log basic request metadata (timestamps, model used, token counts). Prompt and completion are
  not logged by default."
- "We work with all providers to, when possible, ensure that prompts and completions are not logged
  or used for training."

From the [ZDR page](https://openrouter.ai/docs/features/zdr): "OpenRouter itself has a ZDR policy
unless you opt into prompt logging."

The responsibility model is explicit: OpenRouter does not train, and pushes provider-training risk to
the caller's routing configuration.

### 3.2 The three controls

1. **Account-level privacy setting.**
   > "On your account settings page, you can set whether you would like to allow routing to providers
   > that may train on your data."
   > — [privacy and logging](https://openrouter.ai/docs/features/privacy-and-logging)

   The same page notes this restriction applies to provider policies, not to OpenRouter's own data
   handling.
2. **Per-request `provider.data_collection: "deny"`** — "use only providers which do not collect
   user data", and the request errors if nothing qualifies (spec, quoted in 2.3).
3. **ZDR**, account-wide or per request via `provider.zdr: true`. Per the
   [ZDR page](https://openrouter.ai/docs/features/zdr):
   - account-level ZDR can be enforced globally or per model group (Anthropic, OpenAI, Google,
     SpaceXAI, non-frontier);
   - the live list is `GET https://openrouter.ai/api/v1/endpoints/zdr` (**711 endpoints** on
     2026-08-08);
   - "a provider's general policy may differ from the specific policy for a given endpoint", so
     OpenRouter maintains **endpoint-specific** data policies;
   - in-memory caching is not considered data retention;
   - ZDR applies to inference only, not third-party plugins or tools;
   - request-level ZDR is an **OR** with account settings — "they strengthen but can't override
     existing restrictions".

Retention terms are surfaced but **not** used for routing:

> OpenRouter displays each provider's retention details but doesn't use retention policies for
> automatic routing decisions. Users must manually select providers matching their data retention
> requirements.
> — [privacy and logging](https://openrouter.ai/docs/features/privacy-and-logging)

`GET /api/v1/providers` returns per-provider `privacy_policy_url`, `terms_of_service_url`,
`status_page_url`, `headquarters`, and `datacenters`, which is a usable machine-readable index of the
first-party terms. For the routed providers relevant here (fetched 2026-08-08): Anthropic HQ `US`,
terms `https://www.anthropic.com/legal/commercial-terms`, status `https://status.anthropic.com/`;
OpenAI HQ `US`, status `https://status.openai.com/`; Google (`google-vertex`) HQ `US`, status
`https://status.cloud.google.com/...`; Amazon Bedrock HQ `US`, status
`https://health.aws.amazon.com/health/status`; Azure HQ `US`, status `https://status.azure.com/`. All
five return `datacenters: null` — i.e. **OpenRouter surfaces no datacenter list for the frontier
providers**.

### 3.3 ZDR versus structured output — verified conflict

`GET /api/v1/endpoints/zdr` on 2026-08-08, filtered to the candidates:

| Model | ZDR endpoints | Of those, with `structured_outputs` |
| --- | --- | --- |
| `anthropic/claude-haiku-4.5` | `google-vertex/global`, `google-vertex/europe`, `google-vertex/us-east5`, `amazon-bedrock/global`, `amazon-bedrock/eu-west-1` | **only the two `amazon-bedrock/*`** |
| `anthropic/claude-sonnet-5` | `amazon-bedrock/global`, `amazon-bedrock/us-east-1`, `google-vertex/global`, `google-vertex/europe`, `google-vertex/us`, `azure/global` | `amazon-bedrock/*` and `azure/global` |
| `google/gemini-3.5-flash-lite` | `google-vertex/global`, `google-vertex/global/flex`, `google-vertex/global/priority` | all three |
| `openai/gpt-5.6-luna` | `azure`, `azure/eu` | both |

The direct `anthropic` endpoint does **not** appear in the ZDR list for either Claude model on this
date. So a request combining `provider.zdr: true` with `response_format: json_schema` and
`require_parameters: true` on `anthropic/claude-haiku-4.5` resolves to Bedrock, and to Bedrock only.
That has a knock-on cost: Bedrock is on the documented **no-cancellation** list (area 7).

### 3.4 First-party provider terms

- **Anthropic**: "Anthropic may not train models on Customer Content from Services"
  ([commercial terms](https://www.anthropic.com/legal/commercial-terms), fetched 2026-08-08). No
  retention period is stated in the commercial terms themselves; deletion obligations sit in §E.4 and
  the linked DPA.
- **OpenAI**: "data sent to the OpenAI API is not used to train or improve OpenAI models (unless you
  explicitly opt in to share data with us)"; "abuse monitoring logs are generated for all API feature
  usage and retained for up to 30 days" unless legal requirements demand longer; approved customers
  can obtain Zero Data Retention, but "Zero Data Retention ineligible endpoints or capabilities may
  retain application state when used, even if you have Zero Data Retention enabled"
  ([your data](https://developers.openai.com/api/docs/guides/your-data), fetched 2026-08-08).
- **Google Vertex AI**: ~~*not verified*~~ — **resolved 2026-08-12**, see
  [cloud counterparty training and retention terms](./cloud-counterparty-training-and-retention-terms.md) §3.
  The statements were never missing; the docs moved again. `cloud.google.com/vertex-ai/generative-ai/docs/data-governance`
  now redirects to `docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention`,
  and the load-bearing commitments turn out to be **contractual** rather than documentation:
  Service Specific Terms §18, §20(a), §20(h), and GCP ToS §4.3.

Note that these are the **model owners'** terms. When Claude is served through `amazon-bedrock` or
`google-vertex`, the operative terms are AWS's / Google Cloud's service terms, not Anthropic's
commercial terms. That change of legal counterparty is a direct consequence of provider pinning
and should be recorded alongside the pin.

> **Correction (2026-08-12).** Calling it a *substitution* is too simple, and the companion document
> above works the three cloud counterparties out in full. On Bedrock the model vendor's terms are
> **added**, not replaced — AWS Service Terms §50.12.1 incorporates the Anthropic-on-Bedrock EULA
> alongside AWS's own. On Azure with a Claude model the relationship is **inverted**: Claude is a
> Non-Microsoft Product, so Microsoft's no-training clause does not reach it at all and Microsoft
> supplies infrastructure and billing but no prompt-data commitment.

### 3.5 How to guarantee prompts are not trained on

The only defensible construction from primary sources is the conjunction:

1. account privacy setting set to disallow training providers;
2. `provider.data_collection: "deny"` on every request (errors rather than degrades);
3. `provider.zdr: true` on every request;
4. `provider.only` restricted to an endpoint verified present in `/api/v1/endpoints/zdr` at pin time;
5. `provider.allow_fallbacks: false` so no other endpoint can serve the request;
6. the pinned counterparty's own first-party terms read and recorded;
7. post-hoc assertion of `openrouter_metadata` on every response.

Steps 4 and 7 are the ones the docs do not do for you.

---

## 4. Data residency and geographic routing

- **EU in-region routing is enterprise-only.** Endpoint `https://eu.openrouter.ai`; requests are
  "only decrypted within the designated region" and routed exclusively to EU-based providers; data
  never leaves the EU during the request lifecycle
  ([sovereign AI](https://openrouter.ai/docs/guides/features/sovereign-ai), corroborated by
  [privacy and logging](https://openrouter.ai/docs/features/privacy-and-logging)). Availability
  requires contacting the enterprise team.
- `GET /api/v1/models/user` through the EU domain lists the EU-eligible models; the models page has
  an "In-Region Routing" filter.
- **No other region is documented.** The sovereign-AI page addresses EU only.
- For non-enterprise accounts, the documented approximation is provider filtering:
  `GET /api/v1/providers` exposes `headquarters` and `datacenters`, and endpoint tags encode regions
  (`amazon-bedrock/eu-west-1`, `google-vertex/europe`, `azure/eu`, `google-vertex/us-east5`).
  OpenRouter is explicit that independent verification of a datacenter location must come from the
  provider — and as noted in 3.2, `datacenters` is `null` for all five frontier providers.
- `openrouter_metadata.region` reports the OpenRouter edge region that served the request (e.g.
  `"iad"`); this is gateway placement, not inference placement.

**Design consequence.** If beta is US-hosted with no residency commitment, this area imposes nothing.
If a residency commitment is ever made to Players, the only primitive available below the enterprise
tier is endpoint-tag pinning with `allow_fallbacks: false`, and it comes with no OpenRouter-side
guarantee.

---

## 5. Rate limits and 429 semantics

From [limits](https://openrouter.ai/docs/api-reference/limits) and
[limits (current path)](https://openrouter.ai/docs/api_reference/limits), fetched 2026-08-08:

**Credit limits** (402 territory):

- account balance; a negative balance returns 402 "even for free models";
- optional per-key spending caps with reset schedules;
- `GET /api/v1/key` returns `limit`, `limit_reset`, `limit_remaining`, `usage_daily`, `usage_weekly`,
  `usage_monthly`, `is_free_tier`.

**Rate limits** (429 territory):

- Free variants (`:free` suffix): 20 requests/minute; 50 requests/day under $10 lifetime credits,
  1,000 requests/day at $10 or more.
- Cloudflare DDoS protection blocks unreasonable spikes.
- 429 arises from two distinct sources: OpenRouter platform limits, and **upstream provider rate
  limiting** passed through.
- When OpenRouter itself returns 429, the response carries `X-RateLimit-Limit`,
  `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `Retry-After`.
- Recommended handling: exponential backoff, honour `Retry-After`, add fallback models for provider
  diversity.

**No published numeric rate limit exists for paid models.** The docs specify only free-variant caps.
This is the single largest undocumented capacity risk for a centrally-funded beta: a burst of
concurrent Review Sessions on one pinned provider can be 429'd by the upstream provider, and with
`allow_fallbacks: false` there is nowhere for OpenRouter to route it. The mitigation the docs offer
(fallback models) is precisely what pinning forbids.

The [latency and performance](https://openrouter.ai/docs/features/latency-and-performance) page adds
that low balances trigger extra database checks that increase latency, and recommends a minimum
balance of $10–20. For a centrally-funded operator account this argues for a comfortable float and
low-balance alarms, not just budget caps.

---

## 6. Usage and cost reporting

### 6.1 The `usage` field is always present

The [usage accounting](https://openrouter.ai/docs/use-cases/usage-accounting) page states usage data
is included in every response and that `usage: {include: true}` is deprecated. The OpenAPI spec
corroborates twice: there is **no `usage` property on `ChatRequest`**, and `ChatStreamOptions`
contains only `include_usage`, marked deprecated with "This field has no effect. Full usage details
are always included."

`ChatUsage` (spec) fields:

- `prompt_tokens`, `completion_tokens`, `total_tokens`
- `prompt_tokens_details.cached_tokens`
- `completion_tokens_details.{reasoning_tokens, audio_tokens, accepted_prediction_tokens, rejected_prediction_tokens}`
- `cost` — "Cost of the completion" (double, the amount charged to the account)
- `cost_details.{upstream_inference_cost, upstream_inference_prompt_cost, upstream_inference_completions_cost}`
- `is_byok` — "Whether a request was made using a Bring Your Own Key configuration"
- `server_tool_use_details.{tool_calls_requested, tool_calls_executed}`

Token counts are computed with the model's native tokenizer. When streaming, "Usage is always
included in the final chunk".

**Any code written against `usage: {include: true}` should be deleted — it is not in the request
schema at all.**

### 6.2 Exact cost after the fact

`GET /api/v1/generation?id=<gen-id>` (spec + [get a generation](https://openrouter.ai/docs/api-reference/get-a-generation)):

- `total_cost` (required), `usage` (USD), `upstream_inference_cost`, `cache_discount`
- `tokens_prompt` / `tokens_completion`, and `native_tokens_prompt`, `native_tokens_completion`,
  `native_tokens_reasoning`, `native_tokens_cached`, `native_tokens_completion_images`
- `latency` ("Total latency in milliseconds"), `generation_time`, `moderation_latency`
- **`cancelled`** (boolean), `finish_reason`, `native_finish_reason`
- `id`, `model`, `created_at`

The generation id arrives on the response and in the `X-Generation-Id` header for all endpoints.

### 6.3 Aggregates and budgeting

- `GET /api/v1/credits` → `{ total_credits, total_usage }`.
- `GET /api/v1/activity` → rows of
  `{ date, model, model_permaslug, provider_name, endpoint_id, requests, prompt_tokens, completion_tokens, reasoning_tokens, usage, byok_usage_inference }`,
  filterable by `date` (UTC, last 30 days), `api_key_hash`, `user_id` (org accounts), and
  `group_by=workspace`. **Granularity is date × model × endpoint (× workspace × key) — not per
  request**, and the window is 30 days.
- The spec also exposes `/api/v1/keys` (per-key limits), `/api/v1/workspaces/{id}/budgets/{interval}`,
  `/api/v1/analytics/query`, and `/api/v1/observability/destinations`.

**Design consequence for per-Player cost attribution.** OpenRouter gives per-request cost only in the
response's `usage.cost` (or `/generation`). It gives no per-end-user aggregate. The `user` request
field is documented for abuse isolation, not billing. Therefore per-Player and per-Review-Session
cost attribution must be **recorded by the Coach Engine at call time** from `usage.cost` +
`X-Generation-Id`, keyed to the session; the OpenRouter APIs can then only be used to reconcile
totals. Budget enforcement primitives available upstream are per-key limits and workspace budgets —
i.e. the natural shape is one API key (or workspace) per environment, with a hard `limit`, and
per-Player accounting owned locally.

---

## 7. Cancellation, retries, timeouts, streaming

### 7.1 Cancellation and billing — provider-dependent

From [streaming](https://openrouter.ai/docs/api-reference/streaming) (fetched 2026-08-08):

- "For supported providers, this immediately stops model processing and billing."
- Cancellation support varies; roughly 25+ providers support it, and **AWS Bedrock, Groq, and Google
  do not currently support cancellation**.
- "For non-streaming requests or unsupported providers, the model will continue processing and you
  will be billed for the complete response."

`/generation` exposes a `cancelled` boolean for post-hoc confirmation.

**Design consequence, and it interacts badly with area 3.** The ZDR-plus-structured-output pin for
Claude Haiku 4.5 lands on Bedrock, which does not support cancellation. A Player abandoning a Review
Session mid-stream on that pin is billed in full. If abandonment is expected to be common in an
interactive web session, that is a real cost line, and it argues for either (a) short bounded
`max_completion_tokens` on authoring calls, (b) accepting the direct `anthropic` endpoint and losing
ZDR, or (c) choosing `google/gemini-3.5-flash-lite` where the ZDR endpoints are Google — which is
*also* on the no-cancellation list. Only the `openai/gpt-5.6-luna` → `azure` pin plausibly gives ZDR,
structured outputs, and cancellation together; *the cancellation support of the `azure` endpoint
specifically is not stated in the docs and is not verified.*

### 7.2 Retries

OpenRouter's automatic retry surface is exactly the fallback machinery of area 2 — provider fallback
(`allow_fallbacks`) and model fallback (`models`). Disabling both, as pinning requires, means **the
client owns all retry logic**. The limits page recommends exponential backoff honouring
`Retry-After`. Mid-stream failures explicitly "prevent failover since partial content reached the
client" ([errors](https://openrouter.ai/docs/api-reference/errors)).

The existing Grounding Gate's "attempt, ground, retry once, then safe-render" is compatible with
this, provided the retry is a fresh pinned request and both attempts are counted in provenance.

### 7.3 Timeouts

**No numeric request timeout is documented.** The OpenAPI spec defines two timeout error shapes:

- `RequestTimeoutResponse` — HTTP **408**, "Request Timeout - Operation exceeded time limit",
  example message "Operation timed out. Please try again later."
- `EdgeNetworkTimeoutResponse` — HTTP **524**, "Infrastructure Timeout - Provider request timed out
  at edge network", example message "Request timed out. Please try again later."

Both 408 and 524 are listed among the documented responses of `POST /chat/completions`. The duration
that triggers either is not stated anywhere I could find. **Open question.**

### 7.4 Streaming

- Standard SSE. OpenRouter periodically emits SSE comment lines, specifically `": OPENROUTER PROCESSING"`,
  as keepalives; per the SSE spec these must be skipped before `JSON.parse`, which "will cause errors
  if passed to JSON.parse()" ([streaming](https://openrouter.ai/docs/api-reference/streaming)).
- `stream_options.include_usage` is deprecated and inert; usage arrives in the final chunk regardless.
- Pre-stream errors use real HTTP status codes and can trigger automatic failover.
- Mid-stream errors arrive as SSE events carrying an `error` field alongside the normal fields, with
  `finish_reason: "error"`, because the 200 is already committed
  ([errors](https://openrouter.ai/docs/api-reference/errors)).
- Structured outputs stream as valid partial JSON that is complete and schema-conforming only at the
  end ([structured outputs](https://openrouter.ai/docs/features/structured-outputs)).

**Design consequence.** A streaming client must treat `finish_reason: "error"` as a first-class
authoring failure, not as a truncated success — otherwise the Grounding Gate will attempt to ground a
partial draft. And because Response Healing is documented as a **non-streaming** mitigation,
streaming JSON authoring gives up that safety net.

---

## 8. Outage behaviour with fallbacks disabled

With `provider.allow_fallbacks: false` and no `models` array, the documented behaviour is a plain
error, not a substitution:

- `allow_fallbacks: false` → "use only the primary/custom provider, and return the upstream error if
  it's unavailable" (spec).
- With `order` plus `allow_fallbacks: false` → "OpenRouter returns an error instead of routing to a
  provider outside your list"
  ([provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)).
- `order` alone → "If no providers are available, the request will fail with an error message" (spec).
- `data_collection: "deny"` with no qualifying provider → "your request will return an error" (spec).
- **503** is documented as "No provider meets routing requirements"
  ([errors](https://openrouter.ai/docs/api-reference/errors)).

Error shape ([errors](https://openrouter.ai/docs/api-reference/errors)):

```ts
type ErrorResponse = {
  error: { code: number; message: string; metadata?: Record<string, unknown> };
};
```

The HTTP status matches `error.code`, "except when the model begins processing — then it returns 200
OK with error details in the response body or SSE events". The spec adds `openrouter_metadata` and
`user_id` as optional siblings of `error` on the error envelopes.

Documented status codes for `POST /chat/completions` (spec): **200, 400, 401, 402, 403, 404, 408,
413, 422, 429, 500, 502, 503, 524, 529**. Documented meanings:

| Code | Meaning |
| --- | --- |
| 400 | invalid/missing parameters, or CORS |
| 401 | invalid credentials or expired OAuth |
| 402 | insufficient credits (account or per-key) |
| 403 | insufficient permissions or guardrail block |
| 408 | operation exceeded time limit |
| 429 | rate limited (platform or upstream); check `Retry-After` |
| 502 | selected model unavailable or returned an invalid response |
| 503 | no provider meets routing requirements |
| 524 | provider request timed out at edge network |

OpenRouter additionally normalises provider errors into canonical `error_type` values —
`context_length_exceeded`, `max_tokens_exceeded`, `authentication`, `permission_denied`,
`payment_required`, `rate_limit_exceeded`, `provider_overloaded`, `invalid_request`, `invalid_prompt`,
`content_policy_violation`, `refusal`, `server`, `timeout`, `unmapped`. *Where exactly `error_type`
sits in the JSON envelope (top-level vs `metadata`) is not stated on the page I fetched — verify
against a real error response before pattern-matching on it.*

**Design consequence.** This maps cleanly onto the existing vocabulary: every one of these outcomes
is `ProviderUnavailableReason::LanguageLayer`, and the already-built safe-degradation path
("The Language Layer is unavailable. Your earlier review remains intact, and Stockfish exploration
still works.") is the correct terminal state. Pinning converts a class of silent quality regressions
into a class of loud, already-handled outages. That is the right trade for a grounded product, and it
means **pinning costs availability, and the product has already paid for that.**

Live evidence that this matters, 2026-08-08: `openai/gpt-5.4-mini` reported `status: -5` with
`uptime_last_30m` of **35.9%** on all three `openai/*` endpoints while its `azure` endpoint reported
100%. A hard pin to `openai` for that model would have been down for two thirds of the window.
Per-endpoint `status`, `uptime_last_30m`, `uptime_last_5m`, `uptime_last_1d`, `latency_last_30m`, and
`throughput_last_30m` are all available from `/api/v1/models/{slug}/endpoints`, which makes a
pre-flight or periodic health probe cheap to build.

---

## 9. Model revisions, versioning, and deprecation

- `id` is the slug you send; `canonical_slug` is the "Permanent slug for the model that never
  changes" ([models docs](https://openrouter.ai/docs/models)) and is in practice the dated revision.
- Aliases resolve automatically to canonical models.
- Variant suffixes: `:free`, `:thinking`, `:nitro`, `:floor`, `:online`, `:extended`, plus `:batch`
  observed live.
- Deprecation is exposed as `expiration_date`, "Deprecation date for the model endpoint (null if not
  deprecated)". Providers supply `deprecation_date` in ISO 8601 when integrating; OpenRouter's
  provider monitor then surfaces deprecation warnings, and "Models past their deprecation date may be
  automatically hidden from the marketplace"
  ([provider integration guide](https://openrouter.ai/docs/guides/get-started/for-providers)).
- The `~…-latest` slugs are floating and carry no dated revision (see 2.1).

**What is not established.** There is no documented policy statement that a dated permaslug will
remain servable for any minimum period, no documented notice window before removal, and no documented
guarantee that a permaslug's *weights* are frozen (only that the slug string is permanent). The
observable catalogue does retain old dated revisions (`openai/gpt-4o-2024-05-13`,
`openai/gpt-3.5-turbo-0613`, `anthropic/claude-3-haiku`), which is suggestive but is evidence of
practice, not of policy.

**Design consequence.** A pinned candidate must be treated as perishable. The Evaluation Fingerprint
should store `id`, `canonical_slug`, the provider/endpoint tag, and the fetch date, and a scheduled
job should re-read `/api/v1/models` and alarm on a non-null `expiration_date` or a disappeared slug.

---

## 10. Candidate shortlist for Review Session authoring

All figures from `GET https://openrouter.ai/api/v1/models` and
`GET /api/v1/models/{slug}/endpoints` on **2026-08-08**. Prices are USD per million tokens (the API
returns per-token; multiplied by 1e6 here). Re-verify before committing a pin.

The Claude 5 family is listed as `anthropic/claude-opus-5`, `anthropic/claude-sonnet-5`,
`anthropic/claude-fable-5` (canonical `anthropic/claude-5-fable-20260609`), plus
`anthropic/claude-opus-5-fast`. **There is no `anthropic/claude-haiku-5` in the catalogue** — the
newest Haiku listed is `anthropic/claude-haiku-4.5`. `claude-fable-5` is priced at Opus-fast levels
($10/$50) and is not a cost-viable interactive authoring choice.

### Shortlist

**1. `anthropic/claude-haiku-4.5` — recommended primary**

- Permaslug `anthropic/claude-4.5-haiku-20251001`
- **$1.00 in / $5.00 out** per Mtok; context **200,000**; max completion 64,000
- Structured outputs: **yes on `anthropic`, `azure/global`, `amazon-bedrock/*`; NO on any
  `google-vertex/*`**
- Tools: yes on all endpoints. Also supports `temperature`, `top_p`, `top_k`, `stop`, `reasoning`
- ZDR endpoints: `amazon-bedrock/global`, `amazon-bedrock/eu-west-1`, `google-vertex/*`
- Uptime 2026-08-08: 100% (`anthropic`), 99.74% (`amazon-bedrock/global`)
- **Tradeoff.** Cheapest strong instruction-follower with a first-party Anthropic endpoint, and the
  smallest, most predictable latency profile in the shortlist. 200k context is by far the smallest
  here, which is irrelevant given typed minimised evidence. The catch is the provider matrix:
  `provider.only: ["anthropic"]` gets structured outputs and cancellation but **not** ZDR;
  `provider.only: ["amazon-bedrock"]` gets structured outputs and ZDR but **not** cancellation.
  Choose deliberately and record which.

**2. `google/gemini-3.5-flash-lite` — cheapest credible**

- Permaslug `google/gemini-3.5-flash-lite-20260721`
- **$0.30 in / $2.50 out** per Mtok; context **1,048,576**; max completion 65,536
- Structured outputs: **yes on all six endpoints** (`google-ai-studio`, `/flex`, `/priority`,
  `google-vertex/global`, `/flex`, `/priority`). Also `seed` — the only shortlist entry offering a
  seed on every endpoint, which matters to `CriticalMomentGenerationSettings::seed_supported`
- ZDR endpoints: the three `google-vertex/global*` variants
- Uptime 2026-08-08: 99.58% (AI Studio), 99.75% (Vertex)
- **Tradeoff.** Roughly 3× cheaper on input and 2× on output than Haiku 4.5, with uniform structured
  output and seed support, so the pin is much simpler. Against it: Google is on the documented
  no-cancellation list, so abandoned streams are billed in full; and a "lite" tier is the shortlist's
  weakest bet for strict instruction-following and faithfulness under a Grounding Gate. Worth an
  explicit head-to-head on the gate's retry rate before adopting.

**3. `anthropic/claude-sonnet-5` — quality ceiling / escalation tier**

- Permaslug `anthropic/claude-sonnet-5-20260630`
- **$2.00 in / $10.00 out** per Mtok; context **1,000,000**; max completion 128,000
- Structured outputs: yes on `anthropic`, `azure/global`, all `amazon-bedrock/*`; **no on
  `google-vertex/*`**
- ZDR endpoints: `amazon-bedrock/global`, `amazon-bedrock/us-east-1`, `azure/global`,
  `google-vertex/*`
- Uptime 2026-08-08: 100% (`anthropic`), 99.98% (`amazon-bedrock/claude-on-aws`)
- **Tradeoff.** Cheaper per input token than Opus 5 by 2.5× and the strongest faithfulness bet in the
  shortlist. 2×/2× the cost of Haiku 4.5 and materially slower. The honest use is as an escalation
  tier for Review Moment comments that fail the Grounding Gate on the primary, or as the offline
  quality reference the cheaper pins are measured against — not as the default interactive path.
  `azure/global` is the one endpoint here that is simultaneously ZDR and structured-output capable
  and not a documented no-cancellation provider.

**4. `openai/gpt-5.6-luna` — cost outlier, evaluate before trusting**

- Permaslug `openai/gpt-5.6-luna-20260709`
- **$0.10 in / $0.60 out** per Mtok; context **1,050,000**; max completion 128,000
- Structured outputs: yes on `openai`, `openai/flex`, `openai/priority`, `azure`, `azure/eu`; **no on
  `amazon-bedrock/us-east-1`** (that endpoint offers tools only)
- ZDR endpoints: `azure`, `azure/eu`
- Uptime 2026-08-08: 98.16% (`openai`), 100% (`azure`)
- **Tradeoff.** Ten times cheaper on input and four times cheaper on output than Haiku 4.5, with the
  only shortlist combination that plausibly gives ZDR + structured outputs + cancellation together
  (`azure`). But it is a brand-new, unfamiliar tier with the lowest observed uptime on its primary
  endpoint, no `temperature` in `supported_parameters` on any endpoint, and no track record for
  strict grounding behaviour. Treat it as a candidate to be *earned* through the evaluation harness,
  not as a default.

Deliberately excluded: `openai/gpt-5.4-mini` ($0.75/$4.50) — its three `openai/*` endpoints reported
`status: -5` and 35.9% uptime on the fetch date; `anthropic/claude-fable-5` and
`anthropic/claude-opus-5*` — $10/$50 and $5/$25, an order of magnitude over budget for a
centrally-funded interactive path; every `~…-latest` alias — no dated revision, unfit for a
reproducible fingerprint; every `:free` variant — subject to the 20 RPM / 50–1000 RPD caps in area 5.

### Shared pin shape

For all four, the authoring call should carry: dated-verified `model` slug, `provider.only` naming
one endpoint family, `provider.allow_fallbacks: false`, `provider.require_parameters: true`,
`provider.data_collection: "deny"`, `provider.zdr: true` where the chosen endpoint supports it, a
bounded `max_completion_tokens`, `response_format.json_schema.strict: true`, no `models` array, no
`route`, and no `:` variant suffix. Every response should be checked against
`openrouter_metadata` before the draft reaches the Grounding Gate.

---

## 11. Vertex AI contract on the pinned route (fetched 2026-08-21)

The pin of ADR 0050 rides `google/gemini-3.5-flash-lite-20260721` → `google-vertex/global` through
OpenRouter. Vertex is the unchecked first-party source for 429 semantics, backoff, and request
timeouts. Every number Vertex publishes is **per GCP project**, and the project is **OpenRouter's**,
not ChenChess's. Vertex can give the error contract. It can never give an RPM, TPM, or concurrency
figure that applies to our traffic.

### 11.1 429 is `RESOURCE_EXHAUSTED`

[API errors](https://cloud.google.com/vertex-ai/generative-ai/docs/model-reference/api-errors)
(fetched 2026-08-21) maps HTTP **429** to canonical `RESOURCE_EXHAUSTED`. Causes, in Vertex's
words:

1. API quota over the limit.
2. Server overload due to shared server capacity.
3. The daily limit for requests using `logprobs`.

The example is "Gemini API exceeds request per minute limit." The prescribed client action is
"Retry after a few seconds"; if the error persists for hours, contact support; consider
Provisioned Throughput.

[Error code 429](https://cloud.google.com/vertex-ai/generative-ai/docs/error-code-429) (fetched
2026-08-21) adds the pay-as-you-go message `Resource exhausted, please try again later.` and the
Provisioned Throughput message `Too many requests. Exceeded the Provisioned Throughput.` Without a
Provisioned Throughput subscription, a 429 means reserved capacity was unavailable; the request
may be retried and is not counted against the SLA error rate. Pay-as-you-go remedies Vertex lists:
prefer the **global** endpoint, truncated exponential backoff, a Quota Increase Request when the
model uses quotas, gradual ramp-up to avoid acceleration limits, or a Provisioned Throughput
subscription.

[Cloud Quotas: Troubleshoot quota errors](https://cloud.google.com/docs/quotas/troubleshoot)
(fetched 2026-08-21) states that exceeding a quota with an HTTP/REST request returns HTTP **429
TOO MANY REQUESTS**. That is the Google Cloud HTTP mapping, not a ChenChess-specific number.

### 11.2 Retry delay: `RetryInfo`, not a published `Retry-After` on Vertex 429

The Vertex generative 429 and API-error pages **do not document sending a `Retry-After` header**.
What they document is backoff:

- Minimum delay **one second**, at most **two** retries, subsequent delays exponential
  ([API errors](https://cloud.google.com/vertex-ai/generative-ai/docs/model-reference/api-errors)).
- [Retry strategy](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/retry-strategy)
  (fetched 2026-08-21) treats HTTP **408**, **429**, and **5xx** as retryable; the Google Gen AI
  SDK defaults are `initial_delay` 1.0 s, `attempts` 5, `exp_base` 2, `max_delay` 60 s, with
  jitter. Standard pay-as-you-go: exponential backoff for transient 429s. Priority pay-as-you-go:
  the same, without exceeding quota. Flex pay-as-you-go: do not retry aggressively. Real-time
  chat: fail fast.

Google's API error model still has a typed retry signal. `google.rpc.RetryInfo` in
[`error_details.proto`](https://github.com/googleapis/googleapis/blob/master/google/rpc/error_details.proto)
(fetched 2026-08-21) carries `retry_delay`: "Clients should wait at least this long between
retrying the same request." Clients may ignore the recommendation or retry when the field is
missing; exponential backoff remains recommended. That is the Vertex-side retry contract. It is
not a promise that OpenRouter will forward `RetryInfo` or mint `Retry-After`.

OpenRouter's own 429 (area 5) **does** document `Retry-After` (plus `X-RateLimit-*`) when
OpenRouter itself is the limiter. The Coach Engine therefore parses `Retry-After` at the HTTP
boundary as RFC 9110 allows — delta-seconds or HTTP-date — and, when that header is missing,
`google.rpc.RetryInfo.retryDelay` from the JSON body. A missing or unusable signal uses the
existing 1 s `rate_shaped_retry_delay` floor, doubled per consecutive 429 up to 15 minutes.
It does not treat Vertex's SDK defaults (5 attempts, 60 s cap) as ChenChess policy.

### 11.3 Request timeouts

Vertex documents **504 `DEADLINE_EXCEEDED`**: "The request didn't finish within the deadline. If
the client sets a deadline shorter than the server's default deadline, it might cause 504
errors." The example is a client deadline of 10 seconds
([API errors](https://cloud.google.com/vertex-ai/generative-ai/docs/model-reference/api-errors)).
The **server default duration for Standard or Priority `generateContent` on `global` is not
published**. Flex pay-as-you-go documents a default timeout of **10 minutes**, increasable to
**30 minutes**
([Retry strategy](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/retry-strategy)).
The pin is `google-vertex/global`, not Flex; those Flex numbers do not apply to our traffic and
are recorded only so they are not mistaken for the pin's deadline.

OpenRouter's 408 and 524 shapes (area 7.3) still have **no published fire duration**. That
absence is a finding. Client-side deadlines stay the conservative defaults of ADR 0051 (20 s
attempt ceiling, 10 s comment / 30 s Coach Turn). Observed p50/p95/p99 come from Language Layer
Operational Records, not from saturating the route.

### 11.4 Quotas we cannot use

[Generative AI quotas](https://cloud.google.com/vertex-ai/generative-ai/docs/quotas) (fetched
2026-08-21) lists multimodal `generateContent` input quotas per project, region, base model, and
resolution, plus Flex pay-as-you-go **3 000** requests per minute per base model per project.
Those figures are OpenRouter's project limits. Publishing them here as ChenChess capacity would
be inventing a number we cannot see and cannot raise.

**#331 no longer measures by driving the pinned route into 429/408/524.** Deliberate saturation
is out of scope. `LanguageLayerAdmissionConfig::conservative_defaults()` stays: concurrency 4,
20 s attempt ceiling, 1 s retry delay, 10 s / 30 s deadlines. `allow_fallbacks: false` means
saturation degrades to deterministic safe rendering (comments) or unavailable (Coach Turns) —
a designed outcome, not an outage.

---

## Open questions / not documented

Primary sources did **not** establish the following. Items 1 and 2 are **findings**, not gaps
#331 is expected to close by measurement.

1. **Numeric request timeout (finding).** OpenRouter 408 and 524 error shapes exist in the OpenAPI
   spec; no duration is published for either, and no request-level timeout parameter exists on
   `ChatRequest`. Vertex publishes 504 `DEADLINE_EXCEEDED` when a client deadline is shorter than
   the unpublished server default (area 11.3). Conservative client deadlines stay in force.
2. **Paid-model rate limits (finding).** Only OpenRouter `:free` variant limits are published.
   There is no documented RPM, TPM, or concurrency figure for paid models at any spend level, and
   no documented per-provider pass-through quota. Vertex quota is per OpenRouter's GCP project
   (area 11.4). Coach Engine honours 429 + `Retry-After` and reads observed 429s from Operational
   Records.
3. **Whether `provider.only` accepts full endpoint tags** (`amazon-bedrock/eu-west-1`) or only
   provider slugs (`amazon-bedrock`). The provider-routing page's `"order": ["deepinfra/turbo"]`
   example implies tags work in `order`; `only` is not shown with a tag. Region-level pinning and the
   whole of area 4's non-enterprise story depend on this. **Test before designing around it.**
4. **The `~model-latest` slug family.** Present throughout the live catalogue; I found no docs page
   defining the `~` prefix, its update cadence, or its stability. Treated here as floating aliases on
   the evidence of `canonical_slug == id`.
5. **Permaslug longevity policy.** No documented minimum servable lifetime, notice period, or
   weight-freeze guarantee for a dated slug. Only `expiration_date` surfacing and "may be
   automatically hidden" after deprecation.
6. **Google Vertex AI training and retention terms.** The canonical data-governance URL redirected
   and the fetched page did not contain the operative statements. Since the ZDR-compatible endpoints
   for both Claude candidates include `google-vertex/*`, this gap sits directly on the
   no-training claim.
7. **Cancellation support for the `azure` endpoint family.** The streaming docs name Bedrock, Groq,
   and Google as unsupported and say "25+ providers" support cancellation, without publishing the
   list. The `openai/gpt-5.6-luna` → `azure` recommendation in area 10 rests on this being supported;
   it is unverified.
8. **Exact JSON location of the normalised `error_type`.** The errors page enumerates the canonical
   values but the fetched content did not show the field's position in the envelope.
9. **Whether `strict: true` is enforced by OpenRouter or merely forwarded.** The docs disclaim
   guarantees per endpoint but do not say whether OpenRouter validates the output against the schema
   before returning it, nor what happens on violation (error vs pass-through). Materially affects
   whether the Grounding Gate's JSON parse is the first line of defence or the second.
10. **Response Healing plugin semantics** — cost, latency impact, and whether it can alter content in
    ways that would invalidate grounding provenance. Documented only as a one-line recommendation for
    non-streaming requests. **Do not enable it on grounded authoring until this is answered**, since a
    plugin that rewrites model output sits between the model and the Grounding Gate.
11. **Per-end-user cost attribution upstream.** `/api/v1/activity` aggregates by date × model ×
    endpoint × workspace × key over a 30-day window; the `user` field is documented for abuse
    isolation only. No primary source establishes any per-end-user cost view, so per-Player budgeting
    must be owned locally.
12. **Whether OpenRouter's own metadata logging includes anything beyond "timestamps, model used,
    token counts"** when prompt logging is off, and what `openrouter_metadata.region` implies for
    where the prompt transited. The FAQ sentence is the most specific statement available.
