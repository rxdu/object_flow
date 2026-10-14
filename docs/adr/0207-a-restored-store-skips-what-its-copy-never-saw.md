# ADR-0207: A restored store skips what its copy never saw

- **Status:** **Decided** 2026-10-14 on the author's standing instruction ("go with your recommendations"), the design ADR-0204 decision 9.4 asked for, within ADR-0205 decision 7's scope
- **Date:** 2026-10-14
- **Refines:** ADR-0204 decision 9.4; ADR-0205 decision 7
- **Relates to:** PRD N1; ADR-0197 decision 8 (the recovery point is the last backup); `design/audit/durability.md` B4, B5, B6 and V6; `guides/operate.md` (backups and restore), `guides/wire.md`, `guides/errors.md`

## Context

A restored store holds what its copy held and nothing after it. The live store went on after the copy was made: it made objects, wrote events and drew numbers from the flows' sequences, and integrations kept them. The audit's durability B4 restored a copy and found 20 of 20 object ids an integration kept handed out again, naming new objects, and claim numbers `EXP-12001` to `EXP-12003`, which had been sent to an accounting system as bill references, drawn again. B5 found a subscriber's cursor from before the restore refused only until the restored log passed it, then honoured, so the subscriber skipped 352 of 450 new events. Both are major (ADR-0204 §4: D5, E1, E2).

ADR-0204 decision 9.4 decided that a restore advances every counter the store hands out from past a margin, and left its design to an ADR of its own. ADR-0205 decision 7 widened it to the flows' sequences and to cursors in the gap. This is that design.

Two facts bound it. What the live store handed out after the copy is recorded nowhere the restored store can read: where the restore follows a lost volume or a damaged file, the live store's files are gone with it. And the restore is done by an operator, stopped service in hand, often under pressure: any act it needs can be forgotten, and a forgotten act here fails silently, the restored store serving as though nothing were missing.

## Decision

1. **A copy carries a mark, and a store started with the mark is a restore.** The mark is a row of `of_counter`, `copied`. The backup commands `operate.md` gives write it into each copy they make, closing the copy so the mark is in its file, and the restore's steps give one command that writes it into a copy made any other way, such as a copy of the file taken with the service stopped. The service never writes it into a store it serves. A start that finds it advances the store's counters, records the restore and removes the mark, in one transaction, before anything is served.
2. **The margin is estimated from the store's own history.** Every number the store hands out is drawn by an event it records or by a refused request it records as an attempt: an object's number by its creation's event, a position and a transaction by events, a sequence's number by a creation, refused or not. The live store's lost period is no longer than the copy's age at the restore, from the copy's last recorded event or attempt to the start. The margin is the most events and attempts the store recorded in any period as long as that, doubled for a busier one, and at least 1,000, for a store whose history is shorter than the copy's age.
3. **Each counter is advanced past the margin to a round number.** Event positions, transactions and object numbers each go to the next multiple of 1,000 at or past their value plus the margin, so the restart can be seen in the ids: after one, the next claim is, say, `claim-42001`. Every scope of every sequence is advanced the same way. A scope the copy does not hold may have been started in the lost period, so a scope first drawn after a restore starts past the margins of every restore so far, not at 1.
4. **The restore is recorded.** A new table, `of_restore`, keeps for each restore when it started, the copy's last recorded time, the margin, and each counter's gap, from the value the copy held to the one it was advanced to. The start says the same in one line of its log.
5. **A cursor in a gap is refused.** `pull`, `export` and `acknowledge` refuse `invalid_cursor` any position inside a restore's gap, and `<Type>.attempts` any transaction inside one, as they refuse a cursor the store never handed out; `errors.md` says the cause and the remedy, a `pull` without a cursor or an export from the position the restore's record names. An `.attempt_counts` cursor naming a day from the copy's last day to the restore's is refused too, since the restored store recounts those days. 0.2.0 honoured such a cursor once the log passed it (ADR-0204 §11).
6. **A key the lost period applied is applied again.** The restored store has no record of it, so a retry of that request applies it, under a new id, and the id the first application answered names nothing (`not_found`). That is the recovery point's consequence (ADR-0204 §10: recovery is the last backup), and `operate.md` says so beside the restore.
7. **It lands with the 1.0.0 release entry.** `of_restore` is created by the 1.0.0 migration. A store marked and due an upgrade advances at its upgrade, in the same transaction, since it writes nothing before it.

## Alternatives rejected

- **A start flag, `--restored`, alone.** It is the act most likely to be forgotten, given once by hand where a unit file starts the service; forgotten, B4 returns without a word. A mark the backup writes travels with the copy. The restore's own step to mark a copy remains for copies the guide's commands did not make.
- **A file of high-water marks beside the store, written as the store hands numbers out.** A lost volume takes it with the store, and keeping it current costs the writer a flush.
- **A fixed margin, a million.** It is proportionate to nothing: a store writing a thousand events a day would skip nearly three years of them, and a business number such as an invoice reference would jump by a million, past any fixed width a receiving system holds it in.
- **The operator states the margin.** It asks for what an operator rarely knows, at the moment they are least able to find it.
- **An epoch in every id and position.** It changes the form of every id and cursor `/v1` promises.

## Consequences

- A restore leaves a visible gap in every series of numbers the store hands out, a flow's invoice or claim numbers included. A number is never handed out twice; the gap is recorded with its cause in `of_restore`, from which a business required to account for gaps in its numbering can show them.
- The margin is an estimate. A lost period busier than twice the busiest of its length in the store's history is not covered; the start's log line and `of_restore` state the gap, so an operator can check it against the highest ids and numbers integrations hold.
- An integration holding an id from the lost period finds it `not_found`, and a consumer holding a cursor from it is refused, both of which are signals to reconcile, where 0.2.0 handed out the id again and honoured the cursor.
- The start of a restore reads the times of every event and attempt the store holds, about one pass over the file, once.
- `operate.md`'s backup commands each gain a statement, its restore a step, and `check-guides.py` runs the commands.
