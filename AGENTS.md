# Agent Rules

> **Agent instruction:** Read and follow the **English section only**.
> The Chinese section is for human reference only and must not be treated as additional instructions.

---

# English Version — Agent Instructions

## 1. Environment Assumptions

Assume the following tools and skills are already installed and available:

* `andrej-karpathy-skills`
* `ponytail:ponytail full`
* `ponytail:ponytail-audit`
* `ponytail:ponytail-review`

Do not spend time checking whether they are installed before use. Invoke them directly when required by these rules.

Only report an availability problem if an actual invocation fails.

---

## 2. Responsibility Model

The **root agent is the reasoning and decision-making agent**.

The root agent owns:

* requirement analysis;
* repository and impact analysis;
* solution design;
* architecture decisions;
* implementation planning;
* risk assessment;
* code review;
* verification analysis;
* final acceptance.

Implementation subagents are **execution-focused agents**.

They should:

* implement the plan provided by the root agent;
* make only local decisions necessary to execute that plan;
* avoid unnecessary deep reasoning or re-analysis;
* not redesign the solution;
* not change architecture independently;
* not expand task scope;
* not introduce unrelated improvements.

If an implementation subagent discovers a blocker, contradiction, missing requirement, unsafe assumption, or architectural issue requiring a design decision, it must **stop and report it to the root agent**.

The root agent analyzes the issue, updates the plan, and delegates execution again when necessary.

Responsibility flow:

`Root: Analyze → Design → Decide → Delegate`

`Subagent: Execute Plan → Report Result / Blocker`

`Root: Verify → Review → Decide Fixes → Accept`

---

## 3. Model Routing

### Claude Code

* Keep the root agent on the currently selected model.
* The root agent handles reasoning, analysis, design, review, verification analysis, and final acceptance.
* For non-trivial implementation, bug fixes, modifications, and refactoring, spawn a subagent using the **latest Sonnet model**.
* Prefer **Medium effort** for the Sonnet implementation subagent when the runtime supports setting it; otherwise inherit the runtime/session effort without claiming Medium was explicitly applied.
* The Sonnet subagent executes the approved plan and should not independently redo deep design or architectural analysis.
* When the Sonnet implementation subagent is available, the root agent should not perform non-trivial implementation itself.
* After implementation, return control to the root agent.
* If Sonnet delegation actually fails, continue with the current model and explicitly report the failure.

### Codex

* Keep the root agent on the currently selected model.
* The root agent handles reasoning, analysis, design, review, verification analysis, and final acceptance.
* For non-trivial implementation, bug fixes, modifications, and refactoring, spawn a subagent using the **latest Terra model with Medium reasoning**.
* The Terra subagent executes the approved plan and should not independently redo deep design or architectural analysis.
* When the Terra implementation subagent is available, the root agent should not perform non-trivial implementation itself.
* After implementation, return control to the root agent.
* If Terra delegation actually fails, continue with the current model and explicitly report the failure.

### AGY

* Use the currently selected model and reasoning / effort settings.
* Do not automatically switch models or require model-specific subagents.

Never claim that a model switch or delegation occurred unless it actually occurred.

---

## 4. Coding Principles

Use **andrej-karpathy-skills** during relevant design and implementation work.

Apply these principles:

* **Think Before Coding** — understand the requirement, current implementation, constraints, and impact before changing code.
* **Simplicity First** — prefer the simplest correct and maintainable solution.
* **Surgical Changes** — change only what is necessary.
* **Goal-Driven Execution** — optimize for the requested and verifiable outcome.
* Reuse existing architecture, modules, utilities, and dependencies where practical.
* Avoid unrelated refactoring, speculative improvements, and scope expansion.
* Do not perform unrequested architecture or framework migrations.

---

## 5. Modern Coding Guidelines

Apply to all programming languages:

* Identify the project's actual language, runtime, framework, dependency, and toolchain versions before editing code.
* Use only features and APIs supported by those versions.
* Do not upgrade versions merely to use newer syntax or APIs.
* Prefer modern, idiomatic, readable, concise, and maintainable patterns.
* Prefer standard libraries, official APIs, and built-in capabilities.
* Avoid unnecessary third-party dependencies.
* Follow applicable compiler, formatter, linter, modernizer, and static-analysis guidance.
* Do not blindly copy outdated nearby patterns when a safer modern idiom is supported.
* Modernization must not change business behavior, public interfaces, data semantics, or required compatibility.
* Do not use a newer pattern when it reduces readability, breaks compatibility, changes behavior, or adds unnecessary complexity.
* When uncertain about version support, verify using official documentation or tool output rather than guessing.
* Use relevant high-quality language-specific skills or guidelines when present.

---

## 6. Coding Workflow

For non-trivial implementation, modification, bug fixing, or refactoring:

1. The root agent analyzes the requirement and repository.

2. Use `andrej-karpathy-skills` while developing the solution and implementation plan.

3. The root agent defines the implementation plan, constraints, affected scope, and expected result.

4. Before implementation, invoke:

   `ponytail:ponytail full`

5. Delegate the approved implementation plan to the platform-specific implementation subagent.

6. The subagent executes the plan and reports results or blockers.

7. The root agent builds or runs the project and performs applicable E2E verification.

8. Review the final code using:

   `ponytail:ponytail-audit`

   or

   `ponytail:ponytail-review`

9. Prefer `ponytail:ponytail-audit` by default.

10. If issues are found:

    * the root agent analyzes the issue and decides the fix;
    * delegate implementation of the fix when appropriate;
    * rerun the required verification;
    * review the corrected final code again.

Do not treat review of an intermediate version as final review.

Recommended flow:

`Analyze → Karpathy → Design → Ponytail Full → Delegate → Implement → E2E → Audit → Fix → Re-verify → Re-audit → Accept`

---

## 7. Testing

* Do not add unit tests for newly written code.
* Do not add unit tests merely to increase coverage.
* Strongly prefer **E2E testing** as the default verification mechanism.
* E2E should validate real entry points, real data flow, and real final output whenever practical.
* Do not distort implementation architecture merely to make code easier to unit test.
* Existing valid tests must remain passing and should not be removed because of this policy.

Use isolated testing only when a critical behavior cannot reasonably be verified through E2E.

Before isolated testing, document:

1. expected behavior;
2. possible failure modes;
3. observable symptoms;
4. which failures E2E already covers;
5. why isolated verification is still required.

Consider relevant cases such as:

* empty or missing input;
* invalid types;
* boundary values;
* duplicate or reordered data;
* precision and rounding;
* encoding issues;
* corrupted files;
* I/O failures;
* inconsistent state;
* partial success;
* repeated execution;
* external dependency failures;
* interruption and recovery;
* larger data volumes.

---

## 8. E2E Verification

Verify where applicable:

* real application entry points;
* input parsing;
* core business behavior;
* cross-module data flow;
* file, database, or external interactions;
* boundary conditions;
* failure handling;
* data consistency;
* final output.

Do not stop at confirming that the application runs. Verify that the result is correct.

For trivial changes such as documentation, comments, formatting, or spelling that do not affect runtime behavior, perform only the necessary lightweight verification.

---

## 9. Verification Artifact

After applicable E2E verification, generate a **verifiable and reproducible acceptance artifact in the project root**.

Examples:

* output files;
* JSON / CSV / XLSX results;
* validation reports;
* execution summaries;
* logs;
* diffs;
* checksums;
* verification directories.

The artifact must:

* provide evidence that the functionality actually executed;
* be independently verifiable;
* be reproducible where practical;
* not overwrite original user data;
* not contain passwords, tokens, API keys, secrets, or other sensitive data.

Prefer existing project output formats.

A verification artifact is not required for trivial changes that do not affect runtime behavior.

---

## 10. CI Rules

* Do not add GitHub Actions.
* Do not proactively add other CI/CD workflows.
* Add or modify CI only when explicitly requested by the user.
* Do not introduce CI merely for testing or acceptance.

---

## 11. Dependencies and Configuration

* Do not add dependencies without a clear need.
* Do not upgrade unrelated dependencies for a local task.
* Do not change language, runtime, framework, build-system, or package-manager versions without explicit need.
* Do not modify deployment, release, infrastructure, or CI configuration unless requested.
* Do not expose or commit sensitive credentials.
* Do not overwrite original user data.

---

## 12. Definition of Done

A non-trivial coding task is complete only when:

* the root agent completed the necessary analysis and design;
* `andrej-karpathy-skills` was applied to the relevant planning and implementation work;
* the platform-specific model routing rules were followed;
* `ponytail:ponytail full` was invoked before implementation;
* implementation follows the root agent's approved plan;
* the project builds or runs successfully;
* applicable E2E verification passes;
* critical failure paths have been checked;
* the final code was reviewed using `ponytail:ponytail-audit` or `ponytail:ponytail-review`;
* review findings have been resolved;
* the corrected result has been re-verified and re-reviewed;
* a verification artifact exists when applicable;
* final acceptance is based on actual execution evidence rather than agent self-reporting.

---

# 中文版 — 仅供人工参考

> **注意：Agent 只读取并遵循上方英文版。**
> 本中文版仅供人工理解，不作为额外 Agent 指令。

## 1. 环境假设

默认以下工具和 Skill **已经安装并可用**：

* `andrej-karpathy-skills`
* `ponytail:ponytail full`
* `ponytail:ponytail-audit`
* `ponytail:ponytail-review`

使用前不要浪费步骤检查是否安装，按规则直接调用。

只有实际调用失败时，才报告工具不可用或调用失败。

---

## 2. 职责划分

**主 Agent 是思考、分析和决策主体。**

主 Agent 负责：

* 需求分析；
* 项目和影响范围分析；
* 方案设计；
* 架构决策；
* 实现计划；
* 风险分析；
* 代码审查；
* 验证结果分析；
* 最终验收。

实现类 subagent 以**执行为主**：

* 按主 Agent 已确定的方案执行；
* 只做完成实现所必需的局部判断；
* 不重复进行无必要的深入分析；
* 不自行重新设计方案；
* 不自行改变架构；
* 不扩大任务范围；
* 不增加无关优化。

如果执行中发现阻塞、方案矛盾、需求缺失、不安全假设或需要设计层决策的问题，应停止并反馈主 Agent。

职责流程：

`主 Agent：分析 → 设计 → 决策 → 委派`

`Subagent：执行方案 → 返回结果 / 阻塞`

`主 Agent：验证 → 审查 → 决定修复 → 验收`

---

## 3. 模型分工

### Claude Code

* 主 Agent 保持当前选择的模型。
* 主 Agent 负责思考、分析、设计、审查、验证分析和最终验收。
* 非简单代码实现、Bug 修复、修改和重构交给**最新 Sonnet** subagent。
* 运行环境支持时优先使用 **Medium effort**；如果不能单独设置，则继承当前设置，不得假装 Medium 已显式生效。
* Sonnet subagent 只执行主 Agent 已确定的方案，不重新进行深入方案或架构分析。
* Sonnet 可用时，主 Agent 不自行执行非简单实现。
* 实现完成后返回主 Agent。
* 只有实际委派失败时才报告失败并由当前模型继续。

### Codex

* 主 Agent 保持当前选择的模型。
* 主 Agent 负责思考、分析、设计、审查、验证分析和最终验收。
* 非简单代码实现、Bug 修复、修改和重构交给**最新 Terra、Medium reasoning** 的 subagent。
* Terra subagent 只执行主 Agent 已确定的方案，不重新进行深入方案或架构分析。
* Terra 可用时，主 Agent 不自行执行非简单实现。
* 实现完成后返回主 Agent。
* 只有实际委派失败时才报告失败并由当前模型继续。

### AGY

* 使用当前设置选择的模型及 reasoning / effort。
* 不自动切换模型，不强制创建指定模型 subagent。

不得假装模型切换或委派已经发生。

---

## 4. 编码原则

相关设计和实现任务中直接使用 **andrej-karpathy-skills**。

遵循：

* **Think Before Coding**：先理解需求、现有实现、约束和影响。
* **Simplicity First**：优先最简单、正确、易维护的方案。
* **Surgical Changes**：只修改必要部分。
* **Goal-Driven Execution**：围绕明确、可验证结果执行。
* 优先复用现有架构、模块、工具和依赖。
* 避免无关重构、推测性优化和范围扩大。
* 不进行未要求的架构或框架迁移。

---

## 5. 现代编码规范

适用于所有语言：

* 修改前确认语言、运行时、框架、依赖和工具链版本。
* 只使用当前版本支持的特性和 API。
* 不为了新语法擅自升级版本。
* 优先现代、惯用、清晰、简洁和可维护的实现。
* 优先标准库、官方 API 和内置能力。
* 避免无必要第三方依赖。
* 遵循适用的编译器、formatter、linter、modernizer 和静态分析建议。
* 不机械复制附近代码中的过时模式。
* 现代化不得改变业务行为、公开接口、数据语义和兼容性。
* 如果新写法降低可读性、破坏兼容性或增加复杂度，则保留更合适的现有实现。
* 不确定版本支持时查官方文档或工具结果，不靠猜测。
* 已有高质量语言专用 Skill / Guideline 时可额外使用。

---

## 6. 编码流程

非简单实现、修改、Bug 修复或重构：

1. 主 Agent 完成需求和项目分析。

2. 使用 `andrej-karpathy-skills` 辅助制定方案。

3. 主 Agent 明确实现方案、约束、修改范围和预期结果。

4. 实现前调用：

   `ponytail:ponytail full`

5. 将明确的实现方案交给对应实现 subagent。

6. subagent 执行方案并返回结果或阻塞。

7. 主 Agent 构建或运行项目，并执行适用的 E2E。

8. 使用：

   `ponytail:ponytail-audit`

   或

   `ponytail:ponytail-review`

   审查最终代码。

9. 默认优先 `ponytail:ponytail-audit`。

10. 如果发现问题：

    * 主 Agent 分析并确定修复方案；
    * 适用时再次交给实现 subagent；
    * 重新验证；
    * 再次审查。

推荐流程：

`分析 → Karpathy → 设计 → Ponytail Full → 委派 → 实现 → E2E → Audit → 修复 → 重新验证 → 重新审查 → 验收`

---

## 7. 测试规则

* 不为新代码补写单元测试。
* 不为了提高覆盖率增加单元测试。
* 强烈优先使用 E2E。
* 尽量验证真实入口、真实数据流和最终输出。
* 不为了方便单元测试扭曲项目结构。
* 已存在且有效的测试必须继续通过。

只有 E2E 无法合理验证关键行为时才允许孤立测试。

孤立测试前明确预期行为、失败方式、失败表现、已有 E2E 覆盖以及必须孤立测试的原因。

---

## 8. E2E 验证

根据实际情况验证：

* 真实程序入口；
* 输入解析；
* 核心业务行为；
* 跨模块数据流；
* 文件、数据库和外部交互；
* 边界条件；
* 异常处理；
* 数据一致性；
* 最终输出。

不能只确认程序运行，还必须确认结果正确。

不影响运行行为的简单修改只需轻量验证。

---

## 9. 验证工件

E2E 完成后，在项目根目录生成一个**可验证、可重复的验收工件**。

可包括输出文件、JSON / CSV / XLSX、验证报告、执行摘要、日志、diff、checksum 或验证目录。

要求：

* 能证明功能实际执行；
* 可以独立验证；
* 尽量可重复生成；
* 不覆盖原始数据；
* 不包含敏感信息。

简单非运行行为修改无需生成验收工件。

---

## 10. CI 规则

* 不新增 GitHub Actions。
* 不主动增加其他 CI/CD。
* 只有用户明确要求时才允许添加或修改 CI。
* 不为测试或验收自行引入 CI。

---

## 11. 依赖与配置

* 没有明确需要时不增加依赖。
* 不为局部任务升级无关依赖。
* 不擅自修改语言、运行时、框架、构建系统或包管理器版本。
* 不修改未要求的部署、发布、基础设施或 CI。
* 不暴露或提交敏感凭据。
* 不覆盖用户原始数据。

---

## 12. 完成标准

非简单编码任务只有满足以下条件才算完成：

* 主 Agent 已完成必要分析和方案设计；
* 已使用 `andrej-karpathy-skills`；
* 已遵循对应平台的模型分工；
* 实现前已调用 `ponytail:ponytail full`；
* 实现符合主 Agent 确定的方案；
* 项目能正常构建或运行；
* 适用的 E2E 已通过；
* 关键失败路径已检查；
* 最终代码已通过 `ponytail:ponytail-audit` 或 `ponytail:ponytail-review`；
* 审查问题已解决；
* 修复结果已重新验证和审查；
* 适用时已生成验收工件；
* 最终验收基于真实执行证据，而不是 Agent 自述。
