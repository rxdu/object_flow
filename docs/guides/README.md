# ObjectFlow guides

ObjectFlow is a system of record for business workflows. Your flows are declared in YAML; people and AI agents change objects only through declared transitions, and every change is checked, recorded and measured. These guides show how to write a flow, build an application on it, give it to agents, and run it in production.

## Start here

1. [Quickstart](quickstart.md): run the engine and make your first requests, in about ten minutes.
2. [Concepts](concepts.md): the model on one page: types, states, transitions, rules, actors, verdicts and events.

## By task

| Guide | Read it to… |
|---|---|
| [Write a flow](write-a-flow.md) | write a flow and its examples, check them, and use common patterns. |
| [Build an application](integrate.md) | name actors, make requests, handle refusals, build screens and worklists, react to changes, and publish new versions. |
| [Agents](agents.md) | let an AI agent act through generated tools, and keep it within bounds. |
| [Operate a deployment](operate.md) | install, run under systemd, monitor, back up, upgrade and size the service. |

## Reference

| Reference | Holds |
|---|---|
| [API reference](wire.md) | Every operation, its keys and its answers; values on the wire. |
| [Errors and refusals](errors.md) | Every verdict kind, remedy, reason, refusal and fault code, and what to do about each. |
| [Flow format card](flow-format-card.md) | Every key of a flow and its examples file, on one page. |
| [`wire-v1.json`](wire-v1.json) | The keys each `/v1` operation takes, machine-readable. |

The full specifications are in [`docs/design/`](../design/): [`flow-format.md`](../design/flow-format.md) for flows, [`declaration-syntax.md`](../design/declaration-syntax.md) §8 for expressions, and [`publish-and-import.md`](../design/publish-and-import.md) for publishing and go-live. Why the engine works as it does is in the [decision records](../adr/).

## About these guides

They are written for people and for AI agents alike: each page opens with what it covers, every code the engine answers is listed exactly as it is sent, and every example is real. `scripts/check-guides.py` runs the tutorials' commands against the engine and compares each answer with the page, and fails if a reference page misses a code, a flag or a metric the engine has. It runs in CI with every other gate.
