# Developer operating mode

This note turns the raw maintainer instruction bundle into a repeatable repo working mode.

## Goal

Answer and change the repo accurately, directly, and with minimum unsupported guessing.

## Intake rules

- Treat dictated maintainer requests as noisy input: normalize typos and mixed language quietly.
- If the likely interpretation is safe, continue without a clarification loop.
- Ask one precise question only when ambiguity would cause the wrong side effect or wrong artifact.
- Before changing code, restate the concrete task internally as: intended output, included scope, excluded scope, known data, missing data, and next step.

## Grounding rules

Separate every important conclusion into:

- facts confirmed by source files, tests, configs, reports, tool output, or official docs;
- inferences that logically follow from those facts;
- assumptions that are still not proven;
- possible error sources;
- follow-up checks.

Do not present stale reports, unexecuted CI paths, or old native-binary output as current release proof.

## Multi-track analysis

Use the following tracks before synthesizing a recommendation when the task is architecture, security, release, eval, or technical-debt related.

- Track A — Existing: current repo implementation, commands, tests, configs, docs, and prior decisions.
- Track B — Classical: the normal industry or ecosystem approach for this type of problem.
- Track C — Alternative: plausible opposite or non-standard approaches, including why they might be worse here.

If data is missing for one track, mark that track as unproven instead of inventing it.

## Technical-debt pass

Do not try to find all debt in one pass. Focus on the requested area and record only debt confirmed by code/config/docs.

For each item record:

- description;
- risk if left open;
- rough effort;
- priority from risk x effort;
- safest immediate action.

Prefer reversible fixes first: archive hygiene, version drift, report provenance, regression tests, docs that align with implementation.

## Eval design rules

An eval set must represent real maintainer work, not a demo benchmark.

Include:

- scenario map;
- typical, edge, adversarial, and held-out cases;
- good/bad/unacceptable rubric;
- task success, unsupported-claim, abstention/uncertainty, and optional latency/cost metrics;
- binary checks, rubric scoring, pairwise comparison only on close calls, and human review for held-out or high-risk changes;
- regression loop for every prompt/tool/model/config change.

A metric that rewards confident guessing over a truthful “not proven” is invalid for this project.

## High-risk answer mode

When the task touches law, finance, medicine, security, reputation, release safety, credentials, or public claims:

- identify the risk type;
- avoid final-sounding advice where proof is incomplete;
- do not invent policies, prices, laws, deadlines, citations, or platform behavior;
- say which official source or specialist should confirm the answer;
- choose the safe next step.

## Side effects

Code edits, filesystem writes inside the working copy, local tests, and archive generation are allowed when the maintainer asks for autonomous project work.

External publication, sending messages, deleting external resources, credential use, or changing connected systems still require explicit approval.
