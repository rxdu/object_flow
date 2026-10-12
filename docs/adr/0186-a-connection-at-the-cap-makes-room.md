# ADR-0186: A connection at the cap makes room

- **Status:** **Accepted**, 2026-10-12, under the author's mandate that 1.0.0 waits until every major a production review finds is fixed (ADR-0183 decision 1); built the same day, and reported to the author.
- **Date:** 2026-10-12
- **Refines:** ADR-0184 decision 4 (connections served by hyper with a timer, and a cap)
- **Relates to:** ADR-0183 decision 8 (`/health` and `/metrics`), ADR-0114 (the deployment's boundary), `guides/wire.md`, `guides/integrate.md` §9

## Context

ADR-0184 decision 4 capped the connections the service holds, 512 unless `--max-connections` says otherwise, so that open connections could not exhaust its descriptors. Past the cap, a new connection was closed at once. The third production review's adversary found this was a new way to the same outage (C-1, measured and reproduced, 2026-10-12): connections that send nothing fill the cap, and every connection after them is reset until the idle timeout closes them, `/health` and `/metrics` included. With `--max-connections 8 --idle-timeout 2 s`, eight silent sockets got `/health` a TCP reset; once they had timed out, it answered `200`. A pool that leaks connections, or an agent's stalled ones, do the same over the 30 s timeout. Meanwhile the application is shut out, and so is the operator trying to see why. The peers are trusted, so this is a matter of resilience rather than an outside attack, but it fails exactly when a team is trying to find out what is wrong.

## Decision

1. **At the cap, the oldest connection answering nothing is closed to make room**, and the newcomer is served. "Answering nothing" means no request is being answered on it and no response's body is still being written. A connection answering a request is never closed to make room.
2. **Closing never cuts a request short.** A request takes its connection by a compare-and-swap on the connection's count of requests, and holds it until its response's body has been written or dropped. Making room takes a connection only where that count is zero, and marks it closing in the same compare-and-swap. A request that arrives on a connection marked closing begins nothing and is answered `503 busy: … send the request on another`. So no write is applied whose answer the closing then loses.
3. **Only when every connection is answering a request is a newcomer refused**, and it is told so: its request is read and answered `503 busy: every connection --max-connections allows is answering a request`, then the connection is closed, never reset. At most 16 refusals are written at once, each for a second at most, so a flood past the cap holds at most 16 descriptors beyond it. A connection past those 16 is closed at once, as before.
4. **What the cap does is seen.** `/health` answers `connections_open`, and `/metrics` gives `objectflow_connections_open`, `objectflow_connections_made_room_total` and `objectflow_connections_refused_total`. Each connection closed to make room, and each refused, is said on standard error.

## Alternatives rejected

- **Count a connection toward the cap only from its first byte.** Rejected: a silent connection still holds a descriptor. Not counting it brings back the outage the cap exists for, descriptors exhausted with `/health` unanswered, which the second production review measured.
- **A short timeout for a request's head, apart from the idle timeout.** Rejected as the fix: it narrows the window but does not close it. A peer that opens connections faster than they time out still fills the cap, and setting the timeout below a client's normal pause would close healthy connections.
- **A connection reserved for `/health`, or a second listener.** Rejected: the route is not known until the head is read, which is what a silent connection never sends. A second listener is a second address for every deployment to secure. Making room serves `/health` and the application alike.
- **Close the newest connection, or a random one.** Rejected: the oldest idle connection is the one most likely leaked or stalled. A newcomer is about to send its request.
- **Reset past the cap, as before, when every connection is answering.** Rejected: a reset tells a client nothing, and with every connection busy the service is overloaded, which a `503` says and a client can back off from.
- **Write the `503` directly on the socket.** Rejected: that needs tokio's `io-util` feature, a dependency change the author has not approved. Answering through hyper also reads the request first, so closing does not reset the connection before the client reads why.

## Consequences

- A client's idle keep-alive connection may be closed before the idle timeout when the service is at its cap. HTTP allows a server to close an idle connection at any time, and a client meets the same race at the idle timeout. A request that arrives just as its connection is taken is answered `503 busy` and applied nothing, so sending it again is safe without an idempotency key.
- `guides/wire.md` gains `503 busy` and `/health`'s `connections_open`; `guides/integrate.md` §9 describes the cap, the metrics, and `LimitNOFILE` above the cap plus 16.
- Held by `a_connection_at_the_cap_makes_room_or_is_told_busy`: three silent connections at a cap of three, and `/health` answered `200`; three requests half-sent, and a newcomer told `503 busy`; `/metrics` counting both. Also by `idle_and_half_sent_connections_are_closed`, in which the oldest silent connection is closed at once for a fourth, and by `a_connection_taken_to_make_room_begins_nothing`. Each was mutation-checked: no room made, room made by closing a connection that is answering, a reset instead of the `503`, a take that ignores the closing mark, and a hold never let go.
