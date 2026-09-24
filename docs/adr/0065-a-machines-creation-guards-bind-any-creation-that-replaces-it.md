# ADR-0065: A machine's creation guards bind any creation that replaces it

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refines:** ADR-0020, ADR-0064
- **Refined by:** ADR-0109 — a machine may rely on invariants it requires of its binders rather than copying a path's guards onto a creation, and publishing reports what each creation skips.
- **Answers:** open question 13

## Context

ADR-0064 decided that a binder's own `create` replaces the machine's rather than adding to it, because a generic creation inherited alongside a specific one is a hole in whatever the specific one guarantees. Check 8 then holds the replacement to every required attribute and part, so the **data** obligation survives replacement.

The **authority** obligation does not. A guard on the machine's creation — a screening test, a velocity limit, a capability the lifecycle requires of anyone who brings one of these into existence — simply stops applying to a binder that declares its own creation, and nothing says so. A machine can `require` an attribute, a part and a capability of its binders; it could not require that their creations carry a guard.

**This is not hypothetical.** The system being ported has it in production. A warranty contract is creatable through its own endpoint under `WARRANTY_CREATE` (`wr:app/api/warranty_contracts.py:63`), or automatically as a consequence of completing a delivery under `DELIVERY_COMPLETE` (`wr:app/api/deliveries.py:658`, `wr:app/core/state_registry.py:414-440`). A principal holding `DELIVERY_COMPLETE` and no warranty permission whatever creates warranty contracts. Two creation paths, two authorities, and the weaker one is reachable without the stronger one being named.

It is also the shape §4.2 rejects by name when it argues against an unlisted authority list: a construct that grants by construction what the model asks to be declared.

## Decision

**A machine's creation guards bind every creation of every binder, including one that replaces it.** A replacing creation may add guards. It may not drop them.

Publishing **rejects a replacement when an inherited guard reads an input the replacement does not declare**, naming the guard and the input, so the author restates the condition rather than losing it silently. That case is decidable from the text and is the only one where conjunction is not mechanical.

The report of replaced creations added by ADR-0064 now also names the guards carried over, so the effect is visible in the binder where the replacement was written.

## Alternatives rejected

- **Leave it, and note the limit.** Rejected because the limit is a silent weakening rather than an acknowledged one, and because the first consumer already has the defect in production. Every other open question describes something the model cannot do; this one described something it appears to do and does not.
- **`requires guard <name>`, mirroring `requires capability`.** The machine names the guards a binder's creation must declare. Rejected as opt-in: the default stays unsafe, and the author who most needs the protection is the one who did not think to ask for it.
- **Forbid replacing a creation that carries guards.** Rejected because it makes the guard the thing that blocks the model, and a machine's creation almost always carries at least a capability guard, so this would forbid replacement in practice.

## Consequences

- Two types sharing a lifecycle cannot diverge on who may create them without saying so. If they genuinely need different entry authority, that is what `provides capability` is for: the machine requires a capability name and each binder maps it to its own.
- A machine author gains a way to state an entry condition that survives every binder, which is the property "no path to the data except through declared transitions and their guards" requires at the one place the model was not enforcing it.
- The first consumer's warranty case becomes expressible correctly: the lifecycle's creation guard names the capability, the delivery-completion path supplies it, and a principal who lacks it cannot create a contract through either door.
