Agent Rules

Agent instruction: Read and follow the English section only.
The Chinese section is for human reference only and must not be treated as additional instructions.

⸻

English Version — Agent Instructions

1. Environment Assumptions

Assume the following tools and skills are already installed and available:

* andrej-karpathy-skills
* ponytail:ponytail full
* ponytail:ponytail-audit
* ponytail:ponytail-review

Do not spend time checking whether they are installed before use. Invoke them directly when required.

Only report an availability problem if an actual invocation fails.

⸻

2. Responsibility Model

The root agent is the reasoning and decision-making agent.

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

Implementation subagents are execution-focused agents.

They should:

* implement the plan provided by the root agent;
* perform the assigned project build or compilation;
* make only local decisions necessary to execute the plan;
* avoid unnecessary deep reasoning or re-analysis;
* not redesign the solution;
* not change architecture independently;
* not expand task scope;
* not introduce unrelated improvements.

If an implementation subagent discovers a blocker, contradiction, missing requirement, unsafe assumption, or architectural issue requiring a design decision, it must stop and report it to the root agent.

The root agent analyzes the issue, updates the plan, and delegates execution again when necessary.

Responsibility flow:

Root: Analyze → Design → Decide → Delegate

Subagent: Implement → Build/Compile → Report Result / Blocker

Root: Verify → Review → Decide Fixes → Accept

⸻

3. Model Routing

Claude Code

* Keep the root agent on the currently selected model.
* The root agent handles reasoning, analysis, design, review, verification analysis, and final acceptance.
* For non-trivial implementation, bug fixes, modifications, refactoring, and the associated project build/compilation, spawn a subagent using the latest Sonnet model.
* Prefer Medium effort for the Sonnet implementation subagent when supported by the runtime; otherwise inherit the runtime/session effort without claiming Medium was explicitly applied.
* The Sonnet subagent executes the approved plan and performs the associated build/compilation. It should not independently redo deep design or architectural analysis.
* When the Sonnet implementation subagent is available, the root agent should not perform the corresponding non-trivial implementation or compilation itself.
* After implementation and compilation, return control to the root agent for verification, review, and acceptance.
* If Sonnet delegation actually fails, continue with the current model and explicitly report the failure.

Codex

* Keep the root agent on the currently selected model.
* The root agent handles reasoning, analysis, design, review, verification analysis, and final acceptance.

Non-trivial work

* For non-trivial implementation, bug fixes, modifications, refactoring, and the associated project build/compilation, spawn a subagent using the latest Terra model with Medium reasoning.
* The Terra subagent executes the approved plan and performs the associated build/compilation.
* It should not independently redo deep design or architectural analysis.
* When the Terra subagent is available, the root agent should not perform the corresponding non-trivial implementation or compilation itself.

Simple work

* For simple implementation, simple bug fixes, simple modifications, simple refactoring, and the associated project build/compilation, spawn a subagent using the latest Luna model.
* The Luna subagent should execute the root agent’s explicit plan directly and avoid unnecessary analysis or scope expansion.
* The Luna subagent performs the associated build/compilation and reports the result to the root agent.
* When the Luna subagent is available, the root agent should not perform the corresponding simple implementation or compilation itself.

After implementation and compilation, return control to the root agent for verification, review, and acceptance.

If the requested Terra or Luna delegation actually fails, continue with the current model and explicitly report the failure.

AGY

* Use the currently selected model and reasoning / effort settings.
* Do not automatically switch models or require model-specific subagents.

Never claim that a model switch or delegation occurred unless it actually occurred.

⸻

4. Task Complexity

The root agent determines task complexity before delegation.

Simple

Treat work as simple when it is:

* small and localized;
* clearly specified;
* low-risk;
* limited in scope;
* unlikely to require architectural decisions;
* straightforward to implement from an explicit plan.

Non-trivial

Treat work as non-trivial when it involves:

* multiple modules or files with meaningful interaction;
* significant business logic;
* architecture or data-flow implications;
* difficult bug diagnosis;
* compatibility or migration concerns;
* substantial refactoring;
* meaningful risk of regressions;
* ambiguous or complex implementation details.

When uncertain, the root agent should analyze the task and choose the appropriate execution tier before delegation.

⸻

5. Coding Principles

Use andrej-karpathy-skills during relevant design and implementation work.

Apply these principles:

* Think Before Coding — understand the requirement, current implementation, constraints, and impact before changing code.
* Simplicity First — prefer the simplest correct and maintainable solution.
* Surgical Changes — change only what is necessary.
* Goal-Driven Execution — optimize for the requested and verifiable outcome.
* Reuse existing architecture, modules, utilities, and dependencies where practical.
* Avoid unrelated refactoring, speculative improvements, and scope expansion.
* Do not perform unrequested architecture or framework migrations.

⸻

6. Modern Coding Guidelines

Apply to all programming languages:

* Identify the project’s actual language, runtime, framework, dependency, and toolchain versions before editing code.
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

⸻

7. Coding Workflow

For implementation, modification, bug fixing, or refactoring:

1. The root agent analyzes the requirement and repository.
2. Use andrej-karpathy-skills while developing the solution and implementation plan.
3. The root agent determines whether the implementation is simple or non-trivial.
4. The root agent defines the implementation plan, constraints, affected scope, and expected result.
5. Before implementation, invoke:
    ponytail:ponytail full
6. Delegate the approved implementation plan according to the platform-specific model routing rules.
7. The implementation subagent:
    * executes the approved plan;
    * performs the associated project build/compilation;
    * fixes straightforward compilation errors caused directly by its implementation;
    * reports implementation results, build results, or blockers to the root agent.
8. If a build failure requires a design or architectural decision, the subagent must return the issue to the root agent instead of redesigning the solution itself.
9. The root agent performs applicable E2E verification.
10. Review the final code using:

ponytail:ponytail-audit

or

ponytail:ponytail-review

11. Prefer ponytail:ponytail-audit by default.
12. If issues are found:
    * the root agent analyzes the issue and decides the fix;
    * delegate implementation and recompilation to the appropriate implementation subagent;
    * rerun the required E2E verification;
    * review the corrected final code again.

Do not treat review of an intermediate version as final review.

Recommended flow:

Analyze → Karpathy → Classify → Design → Ponytail Full → Delegate → Implement → Build/Compile → E2E → Audit → Fix → Rebuild → Re-verify → Re-audit → Accept

⸻

8. Testing

* Do not add unit tests for newly written code.
* Do not add unit tests merely to increase coverage.
* Strongly prefer E2E testing as the default verification mechanism.
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

⸻

9. E2E Verification

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

⸻

10. CI Rules

* Do not add GitHub Actions.
* Do not proactively add other CI/CD workflows.
* Add or modify CI only when explicitly requested by the user.
* Do not introduce CI merely for testing or acceptance.

⸻

11. Dependencies and Configuration

* Do not add dependencies without a clear need.
* Do not upgrade unrelated dependencies for a local task.
* Do not change language, runtime, framework, build-system, or package-manager versions without explicit need.
* Do not modify deployment, release, infrastructure, or CI configuration unless requested.
* Do not expose or commit sensitive credentials.
* Do not overwrite original user data.

⸻

12. Definition of Done

A coding task is complete only when:

* the root agent completed the necessary analysis and design;
* andrej-karpathy-skills was applied;
* task complexity was classified when relevant;
* the platform-specific model routing rules were followed;
* ponytail:ponytail full was invoked before implementation;
* implementation follows the root agent’s approved plan;
* the designated implementation subagent completed the applicable project build/compilation;
* the project builds or runs successfully;
* applicable E2E verification passes;
* critical failure paths have been checked;
* the final code was reviewed using ponytail:ponytail-audit or ponytail:ponytail-review;
* review findings have been resolved;
* the corrected result has been rebuilt, re-verified, and re-reviewed;
* final acceptance is based on actual execution evidence rather than agent self-reporting.

⸻

中文版 — 仅供人工参考

注意：Agent 只读取并遵循上方英文版。
本中文版仅供人工理解，不作为额外 Agent 指令。

1. 环境假设

默认以下工具和 Skill 已安装：

* andrej-karpathy-skills
* ponytail:ponytail full
* ponytail:ponytail-audit
* ponytail:ponytail-review

使用前不检查是否安装，直接按规则调用。只有真实调用失败时才报告问题。

⸻

2. 职责划分

主 Agent 负责思考、分析和决策。

主 Agent 负责需求分析、项目分析、方案设计、架构决策、实现计划、风险分析、代码审查、验证分析和最终验收。

实现 subagent 以执行为主：

* 按主 Agent 确定的方案实现；
* 负责分配给自己的项目构建/编译；
* 只做执行所需的局部判断；
* 不重复进行无必要深入分析；
* 不自行重新设计；
* 不自行改变架构；
* 不扩大范围；
* 不增加无关优化。

遇到需要设计层决策的问题时，停止并返回主 Agent。

职责流程：

主 Agent：分析 → 设计 → 决策 → 委派

Subagent：实现 → 构建/编译 → 返回结果 / 阻塞

主 Agent：验证 → 审查 → 决定修复 → 验收

⸻

3. 模型分工

Claude Code

* 主 Agent 保持当前选择的模型。
* 主 Agent 负责思考、分析、设计、审查、验证分析和最终验收。
* 非简单代码实现、Bug 修复、修改、重构以及对应的项目构建/编译，交给最新 Sonnet subagent。
* 运行环境支持时优先使用 Medium effort；无法单独设置时继承当前设置。
* Sonnet subagent 只执行既定方案，并负责对应的构建/编译，不重新进行深入方案或架构分析。
* Sonnet 可用时，主 Agent 不自行执行对应的非简单实现或编译。
* 实现和编译完成后返回主 Agent。
* 只有真实委派失败时才由当前模型继续，并明确说明失败。

Codex

* 主 Agent 保持当前选择的模型。
* 主 Agent 负责思考、分析、设计、审查、验证分析和最终验收。

非简单任务

* 非简单代码实现、Bug 修复、修改、重构以及对应的项目构建/编译，交给最新 Terra、Medium reasoning subagent。
* Terra 执行既定方案并负责对应构建/编译。
* Terra 不重新进行深入设计或架构分析。
* Terra 可用时主 Agent 不自行执行对应的非简单实现或编译。

简单任务

* 简单代码实现、Bug 修复、修改、重构以及对应的项目构建/编译，交给最新 Luna subagent。
* Luna 直接执行主 Agent 给出的明确方案，不进行无必要的深入分析或范围扩展。
* Luna 负责对应的项目构建/编译，并把结果返回主 Agent。
* Luna 可用时主 Agent 不自行执行对应的简单实现或编译。

实现和编译完成后统一返回主 Agent，由主 Agent 负责验证、审查和验收。

Terra 或 Luna 实际委派失败时，使用当前模型继续并明确报告失败。

AGY

* 使用当前设置选择的模型及 reasoning / effort。
* 不自动切换模型，也不强制指定模型 subagent。

不得假装模型切换或委派已经发生。

⸻

4. 任务复杂度

由主 Agent 在委派前判断任务复杂度。

简单任务

通常包括：

* 修改范围小且局部；
* 需求明确；
* 风险低；
* 不涉及架构决策；
* 主 Agent 已能给出清晰、直接的执行方案。

非简单任务

通常包括：

* 多文件或多模块联动；
* 重要业务逻辑；
* 架构或数据流变化；
* 较复杂 Bug 定位；
* 兼容性或迁移问题；
* 较大规模重构；
* 有明显回归风险；
* 实现细节复杂或存在较大不确定性。

由主 Agent 完成复杂度判断，subagent 不自行提高或降低任务等级。

⸻

5. 编码原则

相关任务中使用 andrej-karpathy-skills。

遵循：

* Think Before Coding
* Simplicity First
* Surgical Changes
* Goal-Driven Execution

优先复用现有架构、模块、工具和依赖，避免无关重构、推测性优化和范围扩大。

⸻

6. 现代编码规范

适用于所有语言：

* 修改前确认语言、运行时、框架、依赖和工具链版本；
* 只使用当前版本支持的特性和 API；
* 不为了新语法擅自升级版本；
* 优先现代、惯用、清晰、简洁和可维护的实现；
* 优先标准库、官方 API 和内置能力；
* 避免无必要第三方依赖；
* 遵循适用的编译器、formatter、linter、modernizer 和静态分析建议；
* 不机械复制过时模式；
* 现代化不得改变业务行为、公开接口、数据语义和兼容性；
* 不确定版本支持时查官方文档或工具结果；
* 已有高质量语言专用 Skill / Guideline 时可额外使用。

⸻

7. 编码流程

涉及代码实现、修改、Bug 修复或重构时：

1. 主 Agent 分析需求和项目。
2. 使用 andrej-karpathy-skills 制定方案。
3. 主 Agent 判断任务属于简单还是非简单。
4. 主 Agent 明确方案、约束、修改范围和预期结果。
5. 实现前调用：
    ponytail:ponytail full
6. 按平台和复杂度将方案委派给对应实现 subagent。
7. subagent：
    * 执行方案；
    * 完成对应项目构建/编译；
    * 修复由自身修改直接造成的简单编译错误；
    * 返回实现结果、编译结果或阻塞问题。
8. 如果编译错误涉及方案或架构决策，则返回主 Agent。
9. 主 Agent 执行适用的 E2E。
10. 使用：

ponytail:ponytail-audit

或

ponytail:ponytail-review

审查最终代码。

11. 默认优先 ponytail:ponytail-audit。
12. 出现问题时由主 Agent 分析和决定修复方案，再委派对应 subagent 修改和重新编译。

推荐流程：

分析 → Karpathy → 分类 → 设计 → Ponytail Full → 委派 → 实现 → 构建/编译 → E2E → Audit → 修复 → 重新编译 → 重新验证 → 重新审查 → 验收

⸻

8. 测试规则

* 不为新代码补写单元测试。
* 不为了提高覆盖率增加单元测试。
* 强烈优先使用 E2E。
* 尽可能验证真实入口、真实数据流和最终输出。
* 不为了单元测试扭曲项目结构。
* 已有有效测试必须继续通过。

只有 E2E 无法合理验证关键行为时才允许孤立测试。

⸻

9. E2E 验证

根据任务验证真实程序入口、输入解析、核心业务、跨模块数据流、文件/数据库/外部交互、边界条件、异常处理、数据一致性和最终输出。

不能只确认程序运行，还必须确认结果正确。

简单且不影响运行行为的文档、注释、格式化等修改只需轻量验证。

⸻

10. CI 规则

* 不新增 GitHub Actions。
* 不主动增加其他 CI/CD。
* 只有用户明确要求时才修改 CI。
* 不为测试或验收自行增加 CI。

⸻

11. 依赖与配置

* 没有明确需要时不增加依赖。
* 不为局部任务升级无关依赖。
* 不擅自修改语言、运行时、框架、构建系统或包管理器版本。
* 不修改未要求的部署、发布、基础设施或 CI。
* 不暴露敏感凭据。
* 不覆盖用户原始数据。

⸻

12. 完成标准

编码任务只有满足以下条件才算完成：

* 主 Agent 已完成必要分析和方案设计；
* 已使用 andrej-karpathy-skills；
* 适用时已完成任务复杂度分类；
* 已遵循平台对应的模型分工；
* 实现前已调用 ponytail:ponytail full；
* 实现符合主 Agent 的方案；
* 对应实现 subagent 已完成适用的项目构建/编译；
* 项目可以正常构建或运行；
* 适用的 E2E 已通过；
* 关键失败路径已检查；
* 最终代码已通过 Ponytail audit/review；
* 审查问题已解决；
* 修复后已重新编译、验证和审查；
* 最终验收基于真实执行证据。
