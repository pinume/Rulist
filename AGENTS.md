# Agent Rules

> **Agent instruction:** Follow the **English section only**. The Chinese section is for human reference and is not additional instruction.

---

# English Version — Agent Instructions

## 1. Roles

### Root agent

Owns analysis and decisions:

- requirements, repository, domain, architecture, impact, and risk;
- solution and implementation plan;
- compatibility and migration decisions;
- verification, review, and final acceptance.

### Implementation subagent

Executes the approved plan only:

- implement the assigned scope;
- build/compile the project;
- fix straightforward build errors caused by its own changes;
- do not redesign, expand scope, or add unrelated improvements.

If a blocker requires a requirement, architecture, compatibility, or safety decision, stop and return it to the root agent.

`Root: Analyze → Design → Decide → Delegate`

`Subagent: Implement → Build → Report`

`Root: Verify → Review → Fix/Accept`

## 1. Model Routing

Never claim delegation or model switching unless it actually occurred.

### Claude Code

- Keep the root agent on the selected model.
- Non-trivial implementation, bug fixes, modifications, refactoring, and related build/compilation go to the latest Sonnet subagent.
- Prefer Medium effort when supported; otherwise inherit runtime effort.
- Sonnet executes the approved plan and does not redo deep design.
- If delegation fails, continue with the current model and explicitly report the failure.

### Codex

- Keep the root agent on the selected model.
- **Simple work:** latest Luna subagent.
- **Non-trivial work:** latest Terra subagent with Medium reasoning.
- The subagent implements and builds; the root agent retains analysis, design, verification, review, and acceptance.
- If delegation fails, continue with the current model and explicitly report the failure.

### AGY

Use the currently selected model and effort. Do not force model-specific subagents.

## 1. Complexity

The root agent classifies work before delegation.

- **Simple:** small, localized, clear, low-risk, and unlikely to require architecture decisions.
- **Non-trivial:** meaningful multi-file interaction, important business logic, architecture/data-flow impact, difficult diagnosis, migration/compatibility concerns, major refactoring, or regression risk.

When uncertain, analyze first and choose the safer tier.

## 1. Engineering Principles

Use `andrej-karpathy-skills` for relevant design and implementation work.

### Think Before Coding

Understand the requirement, current implementation, versions, constraints, impact, and verification path before editing.

### Architecture and Domain First

Identify relevant domain boundaries, module responsibilities, dependency direction, data flow, persistence boundaries, and external interfaces. Prefer the existing architecture. Do not design for hypothetical future needs.

### Simplicity First

Prefer the simplest correct and maintainable solution. Avoid unnecessary layers, cleverness, speculative extensibility, and duplicate infrastructure.

### Surgical Changes

Change only what is required. Avoid unrelated refactoring, renaming, formatting, migrations, or scope expansion.

### High Cohesion, Low Coupling

Keep modules focused and interfaces clear. If a file approaches roughly 500 lines, review its boundaries; line count alone does not require splitting.

### Clear Boundaries and Data Flow

Keep protocol, domain, persistence, and presentation models separate when the architecture distinguishes them. Validate and transform data at boundaries. Avoid hidden shared mutable state.

### Reuse Stable Business Semantics

Reuse existing modules, utilities, dependencies, and business rules when they already fit. Do not abstract only because code looks similar. Prefer standard-library and existing project capabilities before adding dependencies.

### Delete Before Compatibility

For internal implementations, private APIs, internal boundaries, and unpublished interfaces, **delete obsolete behavior instead of preserving compatibility layers**.

Do not add by default:

- deprecated shims;
- dual-read/dual-write paths;
- old/new branching;
- shadow implementations;
- compatibility wrappers;
- long-lived migration toggles.

Evaluate compatibility separately only for real external contracts such as public APIs, persisted data, database migrations, public CLI arguments, file formats, protocols, or third-party integrations.

When compatibility is required, define its scope, duration, migration path, and removal condition.

### Security and Failure Design

Treat external input as untrusted. Apply least privilege, protect secrets, and preserve authentication/authorization/isolation boundaries where relevant.

Where relevant, consider idempotency, races, transaction boundaries, timeouts, cancellation, retries, backpressure, cleanup, and partial failure. Never use unbounded retries or silent fallback that hides errors.

### Preserve Maintenance Context

Record non-obvious architecture decisions, compatibility constraints, known limitations, temporary solutions, and removal conditions in appropriate comments, docs, issues, or ADRs. Do not leave context-free TODOs.

## 1. Modern Coding

Before editing, identify the actual language, runtime, framework, dependencies, and toolchain versions.

- Use only supported features and APIs.
- Do not upgrade versions just to use newer syntax.
- Prefer modern, idiomatic, readable, maintainable patterns.
- Prefer standard libraries and official APIs.
- Avoid unnecessary dependencies.
- Follow applicable compiler, formatter, linter, modernizer, and static-analysis guidance.
- Do not copy outdated nearby patterns when a safer supported idiom exists.
- Modernization must not change business behavior, public contracts, data semantics, or required compatibility.
- Verify uncertain version support through official documentation or actual tool output.

## 1. Coding Workflow

For implementation, modification, bug fixing, or refactoring:

1. Analyze requirements, repository, impact, and risks.
2. Use `andrej-karpathy-skills` to develop the solution.
3. Classify the task as simple or non-trivial.
4. Define the plan, constraints, scope, and expected result.
5. Invoke `ponytail:ponytail full` before implementation.
6. Delegate implementation according to model-routing rules.
7. The implementation subagent implements and builds/compiles.
8. Design-level blockers return to the root agent.
9. The root agent performs applicable E2E verification.
10. Review final code with `ponytail:ponytail-audit` by default, or `ponytail:ponytail-review` when appropriate.
11. If issues are found, decide the fix, delegate again, then rebuild, re-verify, and re-review.

`Analyze → Karpathy → Classify → Design → Ponytail Full → Delegate → Implement → Build → E2E → Audit → Fix → Rebuild → Re-verify → Re-audit → Accept`

## 1. Testing and Verification

- Do not add unit tests merely because code is new or to increase coverage.
- Prefer E2E verification through real entry points, real data flow, and real outputs.
- Existing valid tests must remain passing.
- Use isolated testing only when critical behavior cannot reasonably be verified through E2E.

When relevant, verify invalid/empty input, boundaries and precision, duplicate/reordered data, encoding/corruption, I/O and dependency failures, partial state, repeated execution, interruption/recovery, and larger data volumes.

E2E should validate applicable entry points, parsing, core behavior, cross-module flow, external/file/database interaction, failure handling, consistency, and final output.

Do not stop at “it runs”; verify the result is correct. Documentation-only or formatting-only changes need only lightweight verification.

## 1. Change Quality and Constraints

Meaningful changes should be verifiable, observable, recoverable where relevant, and explicit in user-visible or compatibility-affecting behavior.

- Do not add GitHub Actions or other CI/CD unless explicitly requested.
- Do not add dependencies without clear need or upgrade unrelated dependencies.
- Do not change language, runtime, framework, build system, or package manager versions without explicit need.
- Do not modify deployment, release, infrastructure, or CI configuration unless requested.
- Do not expose credentials or sensitive information.
- Do not overwrite original user data.

## 1. Definition of Done

A coding task is complete only when applicable requirements are satisfied:

- analysis and design are complete;
- `andrej-karpathy-skills` was used;
- complexity was classified and model routing followed;
- `ponytail:ponytail full` ran before implementation;
- implementation follows the approved plan;
- the designated subagent completed the applicable build/compilation;
- the project builds or runs successfully;
- applicable E2E passes and results are correct;
- critical failure paths were checked;
- final code passed Ponytail audit/review;
- findings were resolved and fixes were rebuilt, re-verified, and re-reviewed;
- final acceptance is based on actual execution evidence, not agent self-reporting.

---

