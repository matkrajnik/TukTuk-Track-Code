# Agent Orchestration

## Available Agents

ECC agents come either from the `ecc@ecc` plugin (plugin-scoped names, `ecc:<name>`) or from a manual install into `~/.claude/agents/` (bare names, `<name>`). Use the name that matches the install you have; the manual installer strips the `ecc:` prefix from copied files.
They are invoked through the Agent tool with that `subagent_type`:

```text
Agent(subagent_type: "planner", prompt: "...")
```

| Agent | Purpose | When to Use |
|-------|---------|-------------|
| planner | Implementation planning | Complex features, refactoring |
| architect | System design | Architectural decisions |
| code-reviewer | Code review | After writing code |
| security-reviewer | Security analysis | Before commits |
| build-error-resolver | Fix build errors | When `npm run build` fails |
| rust-build-resolver | Fix Rust build errors | When `cargo build` / `cargo check` fails |
| refactor-cleaner | Dead code cleanup | Code maintenance |
| doc-updater | Documentation | Updating docs |
| rust-reviewer | Rust code review | Changes in `src-tauri/**/*.rs` |
| vue-reviewer | Vue code review | Changes in `src/**/*.vue` |
| typescript-reviewer | TypeScript code review | Changes in `src/**/*.ts` |

This project has 20 agents in `.claude/agents/`; the full routing table (agents, commands, skills) is in `CLAUDE.md`. Test-first work uses the `rust-testing` (Rust) and `e2e-testing` (Playwright) skills; there is no `tdd-guide` or `e2e-runner` agent.

## Immediate Agent Usage

No user prompt needed:
1. Complex feature requests - Use **planner** agent
2. Code just written/modified - Use **code-reviewer** agent, plus the language reviewer (**rust-reviewer**, **vue-reviewer**, **typescript-reviewer**)
3. Bug fix or new feature - Write the test first, following the **rust-testing** or **e2e-testing** skill
4. Architectural decision - Use **architect** agent

## Parallel Task Execution

ALWAYS use parallel Task execution for independent operations:

```markdown
# GOOD: Parallel execution
Launch 3 agents in parallel:
1. Agent 1: Security analysis of auth module
2. Agent 2: Performance review of cache system
3. Agent 3: Type checking of utilities

# BAD: Sequential when unnecessary
First agent 1, then agent 2, then agent 3
```

## Delegation Completion Contract

Applies to every agent at every depth (parent, child, grandchild):

1. **Your final message IS the deliverable.** Never end your turn with "waiting for background agents" — a spawned task is not a completed task. Ending your turn while children are running orphans their results (completed children cannot notify a parent whose turn has ended).
2. **If you delegate, you own collection.** Wait for results, integrate them, then return. Fire-and-forget delegation is forbidden.
3. **Decompose only when the work cannot fit in one context.** Do not re-delegate a task already sized for a single agent — depth is an outcome, not a plan.

> Rationale: observed failure mode — research agents followed "Parallel Task Execution" above, spawned children, and returned "waiting" as their final answer. All children completed successfully but their results were orphaned. The parallel rule without a completion contract produces zombie tasks.

## Multi-Perspective Analysis

For complex problems, use split role sub-agents:
- Factual reviewer
- Senior engineer
- Security expert
- Consistency reviewer
- Redundancy checker
