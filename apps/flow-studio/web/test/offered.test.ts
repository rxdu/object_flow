// A creation of another type offered on an object, as each backend answers
// it: naming what it creates and the input it binds, and taken as the page
// takes it, the object given that input (ADR-0240).

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import { OPERATOR, Player } from "../src/session/player";
import { BACKENDS, startServer } from "./server";

const DESK = [
  "module: desk",
  "publishing: { taken_by: [human] }",
  "categories: [live, closed]",
  "types:",
  "  Person:",
  "    description: Someone who books a slot.",
  "    tracking: record",
  "    attributes:",
  "      login: { type: identity, actor_kind: human, unique: true }",
  "    states:",
  "      ON: { category: live }",
  "      LEFT: { category: closed, final: true }",
  "    transitions:",
  "      join: { kind: initial, to: ON, required_inputs: [login] }",
  "      leave: { kind: external, from: [ON], to: LEFT }",
  "  Bot:",
  "    description: An agent that books on someone's behalf.",
  "    tracking: record",
  "    attributes:",
  "      name: { type: identity, actor_kind: agent, unique: true }",
  "    states:",
  "      ON: { category: live }",
  "      RETIRED: { category: closed, final: true }",
  "    transitions:",
  "      make: { kind: initial, to: ON, required_inputs: [name] }",
  "      retire: { kind: external, from: [ON], to: RETIRED }",
  "  Slot:",
  "    description: A slot that may be booked.",
  "    tracking: record",
  "    attributes:",
  "      code: { type: string, optional: true, personal: true }",
  "    states:",
  "      OPEN: { category: live }",
  "      SHUT: { category: closed, final: true }",
  "    transitions:",
  "      open: { kind: initial, to: OPEN, optional_inputs: [code] }",
  "      shut: { kind: external, from: [OPEN], to: SHUT }",
  "      forget: { kind: erasure, inputs: { reason: { type: string } } }",
  "  Booking:",
  "    description: A booking of one slot.",
  "    tracking: record",
  "    attributes:",
  "      slot: { reference: Slot }",
  "    states:",
  "      HELD: { category: live }",
  "      DONE: { category: closed, final: true }",
  "    conditions:",
  "      slot_open: { description: The slot is open., expression: inputs.slot.state == Slot.OPEN, remedy: unreachable_from_here }",
  "    transitions:",
  "      book: { kind: initial, to: HELD, required_inputs: [slot], offered_on: [slot], taken_by: [human], guards: { slot_open: deny } }",
  "      end: { kind: external, from: [HELD], to: DONE }",
].join("\n");

describe.each(BACKENDS)("against the %s", (backend) => {
  let wire: Wire;
  let stop: () => Promise<void>;

  beforeAll(async () => {
    ({ wire, stop } = await startServer(backend));
  }, 300_000);

  afterAll(() => stop?.());

  it("offers a creation on the object it binds, and takes it so", async () => {
    const loaded = await wire.load([{ name: "desk.yaml", text: DESK + "\n" }]);
    expect(loaded.loaded).toBe(true);
    const p = new Player(wire, (await wire.reset()).now);
    const op = { id: OPERATOR };
    const ana = await p.request(
      {
        transition: "join",
        actor: op,
        type: "Person",
        inputs: { login: "ana" },
      },
      "Person.join",
    );
    expect(ana.kind).toBe("satisfied");
    const slot = await p.request(
      { transition: "open", actor: op, type: "Slot" },
      "Slot.open",
    );
    expect(slot.kind).toBe("satisfied");
    if (ana.kind !== "satisfied" || slot.kind !== "satisfied") return;

    const offers =
      (await wire.availability(ana.object_id, slot.object_id)) ?? [];
    const book = offers.find(
      (o) => o.creates === "Booking" && o.transition === "book",
    );
    expect(book?.binds).toBe("slot");
    expect(book?.availability).toBe("available");
    expect(book?.inputs).toEqual({});
    expect(
      offers.filter((o) => o.creates === null).every((o) => o.binds === null),
    ).toBe(true);
    expect(
      offers
        .filter((o) => o.transition === "record")
        .every((o) => o.binds === "subject"),
    ).toBe(true);
    if (!book) return;

    const booked = await p.request(
      {
        transition: book.transition,
        actor: { id: ana.object_id },
        type: book.creates ?? "",
        inputs: { [book.binds ?? "subject"]: slot.object_id },
      },
      "Booking.book",
    );
    expect(booked.kind).toBe("satisfied");
  }, 60_000);
});
