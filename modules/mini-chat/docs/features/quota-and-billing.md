# Feature: Quota, Billing, and Token Estimation (mini-chat)

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-featstatus-quota-and-billing-implemented`

## 1. Feature Context

### 1.1 Overview

This feature defines quota enforcement, token estimation, and usage attribution for Mini-Chat using versioned policy snapshots and deterministic reserve and settlement.

### 1.2 Purpose

Ensure strict per-user limits with no hot-path CCM calls, support tier downgrade, and emit usage events for asynchronous CCM billing.

### 1.3 Actors

| Actor | Role in Feature |
|-------|-----------------|
| Chat user | Initiates a chat turn that consumes quota |
| mini-chat | Enforces quota and emits usage events |
| `minichat-quota-policy` plugin | Provides policy snapshots: model catalog (credit multipliers) and per-user credit limits |
| CCM | Publishes policy snapshots and consumes usage events to debit credits asynchronously |
| LLM Provider | Executes requests and returns actual token usage after completion |

### 1.4 References

- **PRD**: [PRD.md](../PRD.md)
- **Design**: [DESIGN.md](../DESIGN.md)
- **Dependencies**: `minichat-quota-policy`

## 2. Actor Flows (CDSL)

### Quota-enforced chat turn

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-flow-quota-enforced-chat-turn`

**Actor**: Chat user

**Success Scenarios**:
- Turn is allowed (possibly downgraded tier), executed, and settled by actual usage.

**Error Scenarios**:
- Turn is rejected at preflight due to quota exhaustion (`quota_exceeded`).

**Steps**:
1. [ ] - `p1` - Resolve current policy snapshot via `minichat-quota-policy` - `inst-qb-01`
2. [ ] - `p1` - Assemble request context and estimate worst-case usage - `inst-qb-02`
3. [ ] - `p1` - Reserve credits and persist `policy_version_applied` - `inst-qb-03`
4. [ ] - `p1` - Call provider with `max_output_tokens` hard cap - `inst-qb-04`
5. [ ] - `p1` - Settle by actual usage and emit outbox usage event - `inst-qb-05`

## 3. Processes / Business Logic (CDSL)

### Quota, Billing, and Token Estimation

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-algo-quota-billing-token-estimation`

**Input**: Policy snapshot + assembled request context + provider usage (if available)

**Output**: Reserve decision + deterministic settlement + outbox usage event

Below is the full “how it works” scheme, assuming:

- CCM stores **versioned policy snapshots**
- the policy snapshot contains: **model catalog (credit multipliers), global switches**; per-user limits are delivered as a **user-specific allocation** linked to `policy_version`
- the user has **a single wallet in credits specifically for chat**
- mini-chat enforces locally in credits and writes usage events; CCM debits credits asynchronously

From here on we distinguish:

- provider-reported **tokens** (source of truth telemetry)
- debited **credits** (quota enforcement unit)

---

#### 1. What is a policy snapshot from CCM

A snapshot is an immutable, versioned config. Mini-chat always knows “which version was applied”.

Important: the **policy snapshot itself is shared** (typically tenant-wide rules). The **per-user limits** (`limit_4h`, `limit_daily`, `limit_monthly`) are user-specific numbers (an allocation) that CCM derives (e.g. from plan/balance) and delivers alongside / keyed by the `policy_version`.

Why the version matters:

- it makes quota enforcement and settlement deterministic for a given turn, even if CCM publishes a new snapshot mid-stream
- it makes async billing/auditing reproducible: CCM can compute credits using the same policy that was applied when the turn was created

Minimum places to persist the applied version (P1):

- `chat_turn.policy_version_applied`
- `usage_outbox.policy_version_applied` (as part of the outbox payload/event)

Contents (logically):

- `policy_version` (monotonic version)
- `model_catalog`:

  - `model_id`
  - `tier` (premium/standard)
  - `input_tokens_credit_multiplier`
  - `output_tokens_credit_multiplier`
  - `multiplier_display`

- `user_limits` (per user allocation; delivered/derived per-user, but tied to `policy_version`):

  - for each tier:

    - `limit_4h`
    - `limit_daily`
    - `limit_monthly`

    These limits are expressed in the same unit: credits.

NOTE: the user still has a single wallet of credits. Per-tier limits are nested caps:

- standard tier limits act as the overall cap (total spend)
- premium tier limits act as a premium-only sub-cap

There may also be kill switches, but that’s not critical here.

---

#### 2. How mini-chat receives snapshots without polling

Push scheme:

1. CCM publishes a new snapshot (version = V+1) in its storage.
2. CCM calls mini-chat: `POST /internal/policy:notify { policy_version: V+1 }`
3. mini-chat fetches that snapshot version (pull) and applies it atomically.
4. mini-chat keeps `current_policy_version` in memory.

A fallback reconcile job every N minutes is still useful, but it is not in the hot path.

---

#### 3. Core quota idea: count credits, not raw tokens

For each turn, mini-chat estimates spend and later records the actual spend.

##### 3.0. Credit arithmetic (P1)

Credits MUST be represented as integers. In P1 we use `credits_micro` (micro-credits).

`credits_micro` is the smallest accounting unit for credits:

- `1 credit = 1_000_000 credits_micro`

Think of it as “cents”, but for credits.
For display/reporting purposes:

```python
credits = credits_micro / 1_000_000
```

Rationale: this avoids float rounding and lets us support fractional multipliers (e.g. `0.33x`) with deterministic integer math.

We interpret each model multiplier as “credits per 1K tokens”, stored as micro-credits:

- `input_tokens_credit_multiplier_micro`
- `output_tokens_credit_multiplier_micro`

Conversion:

```
credits_micro =
  ceil(input_tokens  * input_tokens_credit_multiplier_micro  / 1000)
+ ceil(output_tokens * output_tokens_credit_multiplier_micro / 1000)
```

`ceil(x / 1000)` is implemented as integer math: `(x + 999) / 1000`.

##### 3.1. Preflight reserve (before calling the LLM)

mini-chat selects `effective_model` (with downgrade if needed) based on the current policy.

Then it computes:

- `estimated_input_tokens` (roughly: text + metadata + retrieved chunks)
- `image_surcharge_tokens` (if images are present; fixed conservative budget)
- `tool_surcharge_tokens` (if tools are present; fixed conservative budget)
- `web_search_surcharge_tokens` (if web_search is enabled; fixed conservative budget)
- `max_output_tokens` (upper bound for the answer)
- model credit multipliers (for the chosen model)

Reserve:

```
reserved_input_tokens =
  estimated_input_tokens
  + image_surcharge_tokens
  + tool_surcharge_tokens
  + web_search_surcharge_tokens

reserved_output_tokens = max_output_tokens

reserved_credits_micro =
  credits_micro(reserved_input_tokens, reserved_output_tokens, multipliers)
```

These are the “quota credits” (credit units used for enforcement).

##### 3.2. Period enforcement (daily / monthly) (P1)

For the chosen tier (premium or standard), mini-chat checks that the tier is available across **all** periods.

For each period:

```
if spent_credits_micro(period) + reserved_credits_micro > limit_credits_micro(period):
  tier unavailable
```

The rule “a tier is available only if the limit exists in all enabled periods” means:

- if daily is ok but monthly is not, the tier is unavailable

In P1, the enabled periods are:

- daily
- monthly

4h and weekly are deferred to P2+.

If premium tier is unavailable, mini-chat attempts to downgrade to standard and repeats calculations with different multipliers and different limits.

If no tier is available — typically return 429 `quota_exceeded`.

##### 3.3. Writing the reserve

If allowed, mini-chat performs a local transaction:

- creates a `chat_turn` in `running` state
- stores:

  - `request_id`
  - `policy_version_applied`
  - `effective_model`
  - `reserved_credits_micro`
  - “period keys” (e.g. which bucket for day/month)

- increments local “reserved” counters for the periods (or “held”) so parallel requests cannot slip through

Only after that it calls the LLM.

---

#### 4. Finalization settlement (after the LLM response)

When the LLM responded, you know actual tokens (Azure API returns `usage`):

- `actual_input_tokens`
- `actual_output_tokens`

Compute:

```
actual_credits_micro =
  credits_micro(actual_input_tokens, actual_output_tokens, multipliers)
```

##### 4.1. Reconciliation: `reserved_credits_micro` → `actual_credits_micro`

At preflight you already persisted the worst-case reserve in `chat_turn.reserved_credits_micro`.
After a successful LLM call (terminal `done`), Azure returns actual usage and mini-chat performs reconciliation:

1. Take the source of truth from the provider:

```
actual_tokens = usage.input_tokens + usage.output_tokens
```

2. Convert to credits:

```
actual_credits_micro =
  credits_micro(usage.input_tokens, usage.output_tokens, multipliers)
```

3. Compute the delta (for transparency and debugging):

```
delta_credits_micro = reserved_credits_micro - actual_credits_micro
```

Interpretation:

- if `delta_credits_micro > 0` — the reserve was more conservative than the actual; the difference is **unfrozen** (returned to the available limit)
- if `delta_credits_micro = 0` — worst-case equals actual; nothing is unfrozen
- if `delta_credits_micro < 0` — **overshoot / underestimation**: actual spend exceeded the reserve. In this case you debit the actual, and the overshoot is reflected as quota overrun from the perspective of `spent` counters.

If usage is unavailable (e.g., orphan/disconnect and no terminal usage), the policy must be deterministic. A typical option:

- if the provider was not called — `actual_credits_micro = 0`
- if the provider was called but usage is unknown — `actual_credits_micro = reserved_credits_micro` (conservative)

Then mini-chat performs atomically, in a single transaction (or equivalently safe):

Here “CAS on `chat_turn.state`” means a conditional `UPDATE` (optimistic concurrency):
you transition the turn into a terminal state **only if** it is still `running`.

SQL-level idea:

- `UPDATE chat_turn ... WHERE turn_id = ? AND state = 'running'`

If `0` rows are affected, another worker already finalized the turn, and the current handler
must not debit quota nor write an outbox event again.

Why this should be a “single transaction”:

- CAS + `reserved/spent` updates + outbox insert must commit together,
  otherwise you can get inconsistencies (terminal turn without quota debit / quota debited without outbox event).

1. CAS on `chat_turn.state` (first terminal wins)
2. update period counters:

  - `spent += actual_credits_micro`
  - `reserved -= reserved_credits_micro` (if you keep reserved separately)

3. write a usage event into the outbox with fields:

  - tenant_id, user_id
  - request_id, turn_id
  - effective_model
  - policy_version_applied
  - actual_input_tokens, actual_output_tokens
  - reserved_credits_micro, actual_credits_micro
  - timestamps

If the LLM crashed or was cancelled:

- The settlement policy MUST be deterministic:

  - if the provider was not called — debit 0, `settlement_method = "released"`
  - if the provider was called and usage was returned — debit actual, `settlement_method = "actual"`
  - if unknown (orphan, disconnect, crash) — debit bounded estimate using `min(reserve_tokens, estimated_input_tokens + minimal_generation_floor)`, `settlement_method = "estimated"`

- The orphan watchdog (P1 mandatory) MUST detect turns stuck in `running` state beyond a configurable timeout (default: 5 min) and finalize them using the same CAS guard and the deterministic formula above. The watchdog MUST write the quota settlement and the `modkit_outbox_events` row in the same DB transaction.

Important: this MUST be deterministic, recorded, and use the identical CAS-guarded finalization path as all other terminal triggers.

---

#### 5. How CCM consumes the usage event (credits)

The CCM consumer reads outbox events (at-least-once), deduplicates by `(tenant_id, turn_id, request_id)`.

For each event it applies the same `policy_version_applied` and treats `actual_credits_micro` as the authoritative amount to debit from the user's chat wallet.

---

#### 6. Where the single wallet lives and why there is no overspend

Since the wallet is “chat-only”, you can map:

- CCM has a user credit balance: `credits_balance`
- CCM derives per-user limits (daily/monthly, per tier) in the snapshot from that balance:

  - e.g., if the monthly budget is 1000 credits, then `monthly_limit_credits` is computed so that worst-case does not exceed 1000 credits

So snapshot limits are a “hard cap”.

##### 6.1. Why there is no overspend without real-time calls to CCM

Because mini-chat physically will not allow reserve to pass if the user has exhausted the credit limit for the period.

The CCM balance will lag by seconds, but there will be no overspend if limits are computed correctly.

---

#### 7. How exceedance is handled for day/month (P1)

In the normal scheme you should not exceed limits, because:

- reserve checks limits before calling the LLM
- reserve uses worst-case (estimate + max_output)
- therefore you “occupy” space in the limit upfront

##### 7.1. What can realistically go wrong

1. **Estimate below actual (overshoot)**: actual tokens exceeded max_output or input estimate
   Solution: hard cap max_output at the provider/request level. Then overshoot does not exist.

2. **Policy changes mid-turn**
   Not a problem because the turn records `policy_version_applied`. It is computed under that version.

3. **Orphan turn**
   If you do not know how much was spent, you either debit `reserved_credits_micro` (conservative), or implement provider-specific handling. This is a policy decision.

##### 7.2. Important rule about periods

- day and month are counted on the same credits scale
- a tier is available only if all periods are available
- on reserve you apply `reserved_credits_micro` across all periods at once
- on commit you convert reserved → spent based on actual

---

#### 8. Can a model be “free” in credits?

If a model had `input_tokens_credit_multiplier = 0` and `output_tokens_credit_multiplier = 0`, then:

- the system would not debit credits
- and a credit-based quota would not be consumed, which effectively creates unlimited usage

Therefore, in a credit-based enforcement model:

- `input_tokens_credit_multiplier > 0` always
- `output_tokens_credit_multiplier > 0` always

Otherwise “free” turns into “free GPU warm-up, congratulations”.

---

#### 9. Very short summary

- CCM publishes versioned policy snapshots: models (credit multipliers), per-user limits in credits for day/month (P1).
- mini-chat applies the snapshot at the turn boundary and stores `policy_version` in `chat_turn`.
- mini-chat reserves worst-case credits and checks day/month limits locally, without CCM RPC.
- After the response, mini-chat commits actual credits and writes a usage event.
- CCM consumes usage and updates the balance.
- “One wallet for chat” is implemented because snapshot limits are derived from balance, while enforcement is in mini-chat. CCM balance may be eventually consistent, but there is no overspend if limits are correct and there is a hard cap on output.

#### 10. Calculation example

Below is a numeric example on two models, with two periods (day/month in P1), worst-case reserve and commit by actual. All numbers are made up, but the mechanics are exactly yours.

##### 10.1. Input data

#### 10.1.1. Models and multipliers

For simplicity, assume input and output multipliers are equal and expressed as **micro-credits per 1K tokens**:

- **Premium model P**: `input_tokens_credit_multiplier_micro = 2_500_000`, `output_tokens_credit_multiplier_micro = 2_500_000` (2.5 credits per 1K tokens)
- **Standard model S**: `input_tokens_credit_multiplier_micro = 1_000_000`, `output_tokens_credit_multiplier_micro = 1_000_000` (1.0 credits per 1K tokens)

#### 10.1.2. User limits (in `credits_micro`)

For simplicity, limits are given directly in `credits_micro` (micro-credits).

**Premium tier limits:**

- day: 22_000_000
- month: 300_000_000

**Standard tier limits:**

- day: 60_000_000
- month: 600_000_000

#### 10.1.3. User consumption before the new request

(already “spent credits”)

Premium spent:

- day: 20_000_000
- month: 200_000_000

Standard spent:

- day: 5_000_000
- month: 40_000_000

#### 10.1.4. User request (turn)

- Estimated text input: `estimated_input_tokens = 1,000`
- Hard cap: `max_output_tokens = 500`
- No images, no tools

Worst-case tokens for reserve:

- `estimated_total_tokens = 1,000 + 500 = 1,500`

---

##### 10.2. Step 1 — Premium

#### 10.2.1. Reserve calculation

```
reserved_credits_micro_premium = 1,500 * 2_500_000 / 1000 = 3_750_000
```

#### 10.2.2. Tier availability check across all periods

We need, for each period:
`spent + reserved <= limit`

**day:** 20_000_000 + 3_750_000 = 23_750_000 > 22_000_000  -> does NOT pass
**month:** 200_000_000 + 3_750_000 = 203_750_000 <= 300_000_000 -> passes

The rule “a tier is available only if ALL periods pass” means:

- Premium tier is unavailable due to a tier-specific limit (e.g. premium daily/monthly), so mini-chat downgrades.

---

##### 10.3. Step 2 — Downgrade to Standard

#### 10.3.1. Reserve calculation

```
reserved_credits_micro_standard = 1,500 * 1_000_000 / 1000 = 1_500_000
```

#### 10.3.2. Period check for standard tier

**day:** 5_000_000 + 1_500_000 = 6_500_000 <= 60_000_000 -> passes
**month:** 40_000_000 + 1_500_000 = 41_500_000 <= 600_000_000 -> passes

All periods pass -> Standard tier is available.

#### 10.3.3. What we write to DB at preflight (reserve)

- `effective_model = Standard`
- `reserved_credits_micro = 1_500_000`
- turn state = running
- local “hold/reserved” counters incremented by 1_500_000 for day/month buckets of the standard tier

After that we call the LLM.

---

##### 10.4. Step 3 — LLM returned, commit by actual

Actual usage from provider:

- `actual_input_tokens = 900`
- `actual_output_tokens = 300`
  Total `actual_total_tokens = 1,200`

#### 10.4.1. Actual credits

Standard multipliers => 1.0 credit per 1K tokens:

```
actual_credits_micro = 1,200 * 1_000_000 / 1000 = 1_200_000
```

#### 10.4.2. Settlement: what happens to counters

You usually have two accounting approaches:

- either `spent` plus separate `reserved`
- or you immediately increment spent at preflight and then correct
  More correct and clearer: keep `reserved` separately.

##### 10.4.2.1. Before commit (after reserve)

Standard:

- reserved(day/month) += 1_500_000
- spent not changed yet (or changed, but logically this is a hold)

##### 10.4.2.2. On commit

- decrease reserved by 1_500_000
- increase spent by 1_200_000
- the difference (300_000) is “unfrozen” and returned to the available limit

---

##### 10.5. Final numbers after commit

Standard spent was:

- day: 5_000_000
- month: 40_000_000

After commit (+1_200_000):

- day: 6_200_000
- month: 41_200_000

Premium spent did not change because we did not use the premium tier.

---

##### 10.6. What if output were maximum

If the model returned `max_output_tokens` and total was 1,500 tokens:

- actual_credits_micro = 1_500_000
- reserved_credits_micro = 1_500_000
- nothing is unfrozen; reserved fully becomes spent

---

##### 10.7. Why this scheme prevents overspend

1. Reserve checks all periods before calling the LLM.
2. Reserve uses worst-case (input estimate + max_output cap).
3. Output is actually limited by the hard cap.
4. Commit corrects to actual and returns the extra.

---

#### 11. Token Estimation and Quota Reservation Strategy (P1)

##### 11.1. Problem

Azure OpenAI (Responses API) **does not provide a pre-execution token estimation mechanism** for multimodal requests (images, tools, web search, file search).
Actual consumption (`usage.input_tokens`, `usage.output_tokens`) is available only after the request completes.

Therefore:

- you cannot get an exact cost before calling the LLM
- a strict-budget system cannot rely on post-factum calculation
- you need your own preflight estimator

---

##### 11.2. P1 goals

1. Guarantee no overspend against limits (daily / monthly).
2. Avoid real-time CCM calls in the hot path.
3. Be deterministic with versioned policy snapshots.
4. Allow multimodal requests without losing budget control.
5. Minimize underestimation even at the cost of some conservativeness.

---

##### 11.3. Overall approach

In P1 we use a two-phase scheme:

#### 11.3.1. Phase A — Preflight Reserve (upper bound)

Before calling the LLM, mini-chat:

1. Assembles the full request context:

  - system prompt
  - history or summary
  - RAG chunks (if applicable)
  - user message
  - metadata/tool wiring

2. Estimates input tokens.
3. Adds budget for:

  - max_output_tokens
  - images
  - tools / web_search

4. Computes `reserved_credits_micro`.
5. Checks limits across all periods.
6. If allowed — persists reserve and calls the LLM.

#### 11.3.2. Phase B — Settlement (by actual)

After receiving a response:

1. Take actual usage from the provider.
2. Recompute actual credits.
3. Perform CAS-settlement:

  - reserved → released
  - actual → charged

4. Write a usage event to the outbox.

---

##### 11.4. Input token estimation

#### 11.4.1. Context must be assembled before reserve

RAG retrieval, history trimming, and system prompt assembly happen BEFORE estimation.
Estimation must be based on the actual final text payload.

---

#### 11.4.2. Estimating the text portion

In P1, conservative estimation without a tokenizer is acceptable.

Example:

```
estimated_text_tokens =
  ceil(utf8_bytes / BYTES_PER_TOKEN_CONSERVATIVE)
  + fixed_overhead_tokens
  + safety_margin
```

Where:

- `BYTES_PER_TOKEN_CONSERVATIVE` = 3 (or an even more conservative value)
- `fixed_overhead_tokens` — a constant from the policy snapshot
- `safety_margin` = 10–30%

Underestimation is unacceptable.
Overestimation is acceptable.

---

#### 11.4.3. RAG

If retrieval is already performed:

- estimate the actual selected chunks.

If retrieval is deferred:

- use a worst-case budget from policy:

  - `max_chunks * max_chunk_tokens`

---

#### 11.4.4. History

History has already been trimmed/summarized before assembly.
Estimation is performed on the real text that will be sent.

---

##### 11.5. Images (Vision)

Azure does not provide a pre-execution estimate for vision tokens.

In P1 we use a fixed surcharge:

```
image_surcharge_tokens = num_images * image_token_budget
```

`image_token_budget` is defined in the policy snapshot and must be conservative (e.g., p95/p99 of historical usage).

Important:

- `image_token_budget` > 0 always.
- Even if the model is “free” in credits, the quota budget for images cannot be 0.

---

##### 11.6. Web Search / Tools

Because the provider may add hidden prompt:

In P1 we use a fixed surcharge:

```
tool_surcharge_tokens = ...
```

or

```
web_search_surcharge_tokens = ...
```

Values are defined in the policy snapshot.

---

##### 11.7. Reserved credits calculation

After estimation:

```
reserved_input_tokens =
  estimated_input_tokens + image_surcharge_tokens + tool_surcharge_tokens + web_search_surcharge_tokens

reserved_output_tokens = max_output_tokens

reserved_credits_micro =
  credits_micro(reserved_input_tokens, reserved_output_tokens, multipliers)
```

Where:

- `input_tokens_credit_multiplier` > 0 (always)
- `output_tokens_credit_multiplier` > 0 (always)

---

##### 11.8. Limit checks

A tier is considered available only if:

```
spent_credits_micro(period) + reserved_credits_micro <= limit_credits_micro(period)
```

for all enabled periods (P1):

- daily
- monthly

Reserve is applied to all periods.

---

##### 11.9. max_output_tokens

`max_output_tokens` must be set as a hard cap in the provider request.

This guarantees:

- you cannot exceed the reserved budget
- deterministic worst-case

---

##### 11.10. Settlement

After the response:

```
actual_credits_micro =
  credits_micro(actual_input_tokens, actual_output_tokens, multipliers)
```

In P1, image/tool actual is typically not recomputed separately: the source of truth for `actual_input_tokens` / `actual_output_tokens` is provider usage (as the provider counts it).

Settlement:

- reserved is released
- actual is charged
- the difference is returned to available limits

---

##### 11.11. Policy versioning

Each turn stores:

```
policy_version_applied
```

Settlement and billing in CCM are performed using that policy version.

Changing policy does not affect already started turns.

---

##### 11.12. Why this works despite having no estimate API

Azure provides exact usage only after execution.
This does not prevent a strict budget system because:

- reserve is worst-case based
- output is limited by a hard cap
- settlement corrects to actual
- underestimation is not allowed
- overestimation is allowed

Thus we achieve:

- no overspend
- no real-time CCM calls
- per-turn determinism
- multimodal compatibility

---

##### 11.13. Key P1 invariants

1. Context assembly happens before reserve.
2. max_output_tokens is always limited.
3. input/output credit multipliers are > 0.
4. Reserve is computed in credits.
5. Reserve > Settlement never produces negative limits.
6. policy_version is fixed on the turn.
7. **Replay is side-effect-free**: when a completed turn is replayed for the same `(chat_id, request_id)`, the system MUST NOT take a new quota reserve, MUST NOT update `quota_usage` or debit credits, MUST NOT insert a new outbox row, and MUST NOT emit audit or billing events. Replay is a pure read-and-relay operation.
8. **Outbox emission is atomic with settlement**: the CAS-guarded finalization transaction MUST include the outbox row insert (`modkit_outbox_events`) in the same DB transaction as quota settlement. It MUST be impossible for quota to be debited without a corresponding outbox row.
9. **Orphan watchdog is P1 mandatory**: a periodic background job MUST detect turns stuck in `running` state beyond a configurable timeout (default: 5 min) and finalize them with a bounded best-effort debit using the same CAS guard, quota settlement, and outbox emission as all other finalization paths. The watchdog ensures no turn can permanently evade billing. See DESIGN.md `cpt-cf-mini-chat-component-orphan-watchdog`.

---

This is a complete model for Mini-Chat P1, compatible with:

- CCM snapshots
- per-period limits
- multimodal requests
- strict quota enforcement without a provider estimate API.

## 5. Definitions of Done

### Persist policy version per turn and propagate to outbox

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-dod-policy-version-per-turn`

The system **MUST** persist `policy_version_applied` on `chat_turn` and include the same value in the usage outbox payload for deterministic settlement and async CCM billing.

**Implements**:
- `cpt-cf-mini-chat-flow-quota-enforced-chat-turn`

### Enforce preflight reserve and tier downgrade across all periods

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-dod-preflight-reserve-and-downgrade`

The system **MUST** reserve worst-case credits before calling the provider, enforce daily/monthly limits locally, and downgrade from premium to standard when premium is unavailable.

**Implements**:
- `cpt-cf-mini-chat-flow-quota-enforced-chat-turn`

### CAS-guard settlement and emit outbox usage exactly once

- [ ] `p1` - **ID**: `cpt-cf-mini-chat-dod-cas-guarded-settlement-and-outbox`

The system **MUST** finalize turns using CAS on `chat_turn.state` and MUST emit at most one outbox usage event per turn.

**Implements**:
- `cpt-cf-mini-chat-flow-quota-enforced-chat-turn`

## 6. Acceptance Criteria

- [ ] If premium tier is exhausted in any period, the system downgrades to standard (if available) before calling the provider.
- [ ] If no tier is available, the system rejects at preflight with `quota_exceeded` and does not call the provider.
- [ ] The system persists `policy_version_applied` per turn and uses the same version for settlement and outbox emission.
- [ ] The system enforces a hard cap on `max_output_tokens` to prevent overshoot beyond reserved worst-case.
- [ ] Replaying a completed turn for the same `(chat_id, request_id)` MUST NOT take a new quota reserve, debit credits, or emit a new outbox row (replay is side-effect-free).
- [ ] The orphan watchdog MUST finalize turns stuck in `running` state beyond the configured timeout using the same CAS guard, quota settlement, and outbox emission as all other finalization paths (P1 mandatory).
- [ ] For any committed quota debit, there MUST exist exactly one corresponding `modkit_outbox_events` row (written in the same DB transaction as the settlement).
