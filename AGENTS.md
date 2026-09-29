Agent Engineering Guidelines

1. General Rules and Responsibilities

• Never claim that a model switch, delegation, build, or test has occurred unless it actually did; conclusions must be based on real execution evidence, not Agent self-reporting.
• If a ponytail skill is unavailable, state that clearly and continue with equivalent steps. “Latest” model means the latest model available in the runtime.
• Root Agent: owns requirements, architecture, solution design, risk, compatibility, verification, review, and acceptance. When delegation is required, provide the Subagent with a detailed, explicit, directly executable plan whenever possible, including the objective, change scope, constraints, key steps, forbidden changes, and verification method, minimizing the need for Subagent inference.
• Subagent: only executes the plan approved and assigned by the Root Agent, including implementation, build/compilation, and straightforward fixes for build errors directly caused by its own changes, provided those fixes do not involve design, interfaces, dependencies, or scope changes. It must not redesign the solution, expand scope, or make unrelated improvements. If it encounters a requirement, architecture, compatibility, or security decision, or a build failure outside the scope above, it must stop and return the issue to the Root Agent.

2. Model Routing

|Environment|Execution                                 |
|-----------|------------------------------------------|
|Claude Code|Latest Sonnet subagent                    |
|Codex      |Latest Luna subagent                      |
|AGY        |Root Agent executes the task plan directly|

• Claude Code: all implementation tasks are delegated to the latest Sonnet subagent. The Root Agent owns analysis, design, and decisions, and should provide a detailed, explicit, directly executable plan whenever possible, including the objective, change scope, constraints, key steps, forbidden changes, and verification method. When the runtime supports an independent effort setting, prefer Medium; otherwise inherit the runtime setting.
• Codex: all implementation tasks are delegated to the latest Luna subagent. The Root Agent owns analysis, design, and decisions, and should provide a detailed, explicit, directly executable plan whenever possible, including the objective, change scope, constraints, key steps, forbidden changes, and verification method.
• AGY: Subagents are not required. The Root Agent directly implements, builds, and verifies the approved task plan.

If Claude Code or Codex delegation actually fails, the current Root Agent continues and explicitly reports the failure. Audit is still required, and the final report must note that implementation and review were performed by the same model.

3. Workflow

1. Analyze requirements, current state, versions, constraints, impact, and risks. If ambiguity would materially affect behavior, interfaces, data, compatibility, or scope, clarify with the user or present the relevant options first. For local implementation details that do not affect the result, make the smallest reasonable decision consistent with the existing architecture.
2. Use andrej-karpathy-skills to assist solution design. Define the scope and verifiable success criteria; for multi-step tasks, express the plan as “step → verification method”.
3. Before implementation, invoke ponytail:ponytail full.
4. Execute according to Section 2: in Claude Code / Codex, the Root Agent gives the designated Subagent a detailed, executable plan and the Subagent implements it and performs the build/compilation; in AGY, the Root Agent executes directly. Any design-level blocker must be returned to the Root Agent; the Subagent must not revise the plan independently.
5. The Root Agent performs applicable E2E verification.
6. Review the final code with ponytail:ponytail-audit; use ponytail:ponytail-review when supplemental or second-pass review is needed.
7. If issues are found, the Root Agent decides the fix, then the implementation is updated, rebuilt, re-verified, and re-reviewed. If the issue cannot be resolved without a new design decision or further progress is blocked, report the blocker to the user.

Lightweight path: documentation-only or formatting-only changes require only the necessary lightweight verification and may skip Steps 2–6 and delegation.

4. Engineering Principles

• Think Before Coding: before changing code, establish the requirement, current state, versions, constraints, impact, and verification method. State assumptions explicitly. Clarify uncertainty that would materially affect behavior, interfaces, data, compatibility, or scope; otherwise make the smallest reasonable local decision consistent with the existing architecture. Point out a simpler approach when one exists, and push back when appropriate. If a critical fact is unclear, stop and state the blocker.
• Follow the Existing Architecture: when relevant, identify domain boundaries, module responsibilities, dependency direction, data flow, persistence boundaries, and external interfaces. Do not design for hypothetical future requirements.
• Simplicity First: solve the problem with the least code necessary. Do not add unrequested features, abstractions for one-off code, unrequested flexibility or configurability, or defensive logic for hypothetical scenarios without realistic evidence. Error handling should match real risks and system boundaries. If 200 lines were written where 50 would suffice, rewrite it. Ask: “Would a senior engineer consider this over-engineered?”
• Surgical Changes: every changed line should be traceable to the requirement. Do not “clean up” adjacent code, comments, or formatting; do not refactor code that is not broken; do not perform unrelated renames or migrations. Follow the existing style. If unrelated dead code is discovered, report it but do not remove it. However, unused imports, variables, or functions introduced by your own changes must be cleaned up.
• Goal-Driven Execution: convert the task into verifiable goals and iterate until they pass. “Add validation” → verify invalid input is rejected. “Fix bug” → reproduce through a real entry point whenever possible; if reliable reproduction is not possible, document the observed behavior, evidence, and expected behavior, then fix and verify using the closest practical approximation of the real scenario. “Refactor” → behavior remains unchanged and existing tests pass before and after. Follow Section 6 for verification.
• High Cohesion, Low Coupling: keep responsibilities clear and interfaces explicit. When a file approaches 500 lines, inspect module boundaries, but do not split files mechanically based on line count alone.
• Clear Boundaries: do not mix protocol, domain, persistence, and presentation models. Perform validation and transformation at boundaries. Avoid hidden shared mutable state.
• Reuse Existing Capabilities: prefer existing modules, utilities, dependencies, and business rules when they meet the need. Do not abstract merely because code looks similar. Before adding a dependency, check the standard library and existing project capabilities first.
• Delete Before Compatibility: when refactoring internal implementations, private APIs, or unpublished interfaces, delete the old implementation directly. Do not add deprecated shims, dual read/write paths, old/new branches, shadow implementations, compatibility wrappers, or long-lived migration toggles. Evaluate compatibility separately only for real external contracts such as public APIs, persisted data, database migrations, public CLI arguments, file formats, protocols, or third-party integrations, and explicitly define the scope, lifetime, migration path, and removal condition.
• Security and Failure Handling: treat external input as untrusted by default. Apply least privilege and protect sensitive information. Preserve authentication, authorization, and isolation where relevant, and consider idempotency, races, transaction boundaries, timeouts, cancellation, retries, backpressure, resource cleanup, and partial failure. Do not use unbounded retries or silent fallbacks that hide errors.
• Preserve Context: record non-obvious architectural decisions, compatibility constraints, known defects, temporary solutions, and removal conditions in comments, documentation, Issues, or ADRs. Do not leave TODOs without context.

5. Modern Coding

• Before editing, identify the actual language, runtime, framework, dependency, and toolchain versions. If uncertain, verify using official documentation or tool output.
• Use only features supported by the current versions. Do not upgrade versions merely to use newer syntax.
• Prefer standard libraries, official APIs, and modern idiomatic patterns. Follow applicable compiler, formatter, and linter guidance. Do not mechanically copy outdated local patterns.
• Modernization must not change business behavior, public contracts, data semantics, or required compatibility.

6. Testing and Verification

• Do not add unit tests by default merely to increase coverage. Reproduce bugs through real entry points whenever possible. If reliable reproduction is not possible, document the observed behavior, evidence, and expected behavior, then verify the fix using the closest practical approximation of the real scenario. Prefer E2E verification of real entry points, real data flow, and real results. Use isolated tests only when critical behavior cannot reasonably be verified through E2E. Existing valid tests must continue to pass.
• Where applicable, E2E should cover real entry points, parsing, core business behavior, cross-module data flow, file/database/external interactions, failure handling, consistency, and final output.
• Where applicable, check empty/invalid input, boundaries and precision, duplicate/reordered data, encoding/corrupted files, I/O and dependency failures, partial state, repeated execution, interruption and recovery, and larger data volumes.
• Do not stop at confirming that the program “runs”; verify that the result is correct.

7. Change Constraints

• Changes should be verifiable, observable, and recoverable where applicable. Behavior affecting users or compatibility must be designed explicitly.
• Unless explicitly requested, do not add CI/CD, add or upgrade dependencies, change language/runtime/framework/build-system/package-manager versions, or modify deployment, release, or infrastructure configuration.
• Do not expose credentials or sensitive information. Do not overwrite original user data.

8. Definition of Done

Review checks code issues; acceptance checks this list. A task is complete only when all applicable conditions are satisfied:

• Analysis and solution design are complete, assumptions and success criteria are explicit, and model-routing rules were followed.
• Every code change is traceable to the requirement, with no unrelated changes.
• ponytail:ponytail full was invoked before implementation, and the implementation follows the approved plan.
• The project builds or runs successfully; applicable E2E verification passes with correct results; critical failure paths were checked.
• Final code passed audit/review; findings were resolved; fixes were rebuilt, re-verified, and re-reviewed.
• Acceptance is based on real execution evidence.

Lightweight-path tasks only need to satisfy: the change matches the request and the necessary lightweight verification was completed.
