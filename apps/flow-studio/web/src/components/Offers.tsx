import { useEffect, useState } from "react";
import type {
  Declaration,
  TransitionOffer,
  Verdict,
  Wire,
  WireValue,
} from "../api/wire";
import { explain } from "../session/explain";
import type { Player } from "../session/player";
import { InputForm, type Known } from "./InputForm";

const BADGE: Record<TransitionOffer["availability"], string> = {
  available: "bg-emerald-100 text-emerald-900",
  available_with_input: "bg-sky-100 text-sky-900",
  unavailable: "bg-amber-100 text-amber-900",
};

/** The live offer for one object, as its actor sees it (renderers.md §4,
 * ADR-0152): every requestable transition, available, available once its
 * inputs are given, or refused with the clause and remedy that refuse it.
 * What it offers can be taken, as any caller would request it, and a
 * transition marked proposable can be proposed instead, whatever it offers
 * now, since filing checks neither the state nor the guards (DESIGN.md §9). */
export function Offers({
  wire,
  player,
  objectId,
  actor,
  tick,
  onDone,
}: {
  wire: Wire;
  player: Player;
  objectId: string;
  actor: string;
  tick: number;
  onDone: (v: Verdict) => void;
}) {
  const [offers, setOffers] = useState<TransitionOffer[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<{ key: string; propose: boolean } | null>(
    null,
  );
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [known, setKnown] = useState<Known[]>([]);

  useEffect(() => {
    let live = true;
    void Promise.all([
      wire.availability(actor, objectId),
      player.typeOf(objectId).then((t) => player.decl(t)),
      player.known(),
    ])
      .then(([o, d, k]) => {
        if (!live) return;
        setOffers(o ?? []);
        setDecl(d);
        setKnown(k);
        setError(null);
      })
      .catch((e: unknown) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [wire, player, objectId, actor, tick]);

  async function take(
    o: TransitionOffer,
    inputs: Record<string, WireValue>,
    propose = false,
  ) {
    setOpen(null);
    const recording = o.creates !== null;
    const v = await player.request(
      recording
        ? {
            transition: o.transition,
            actor: { id: actor },
            type: o.creates ?? "",
            inputs: { ...inputs, subject: objectId },
          }
        : {
            transition: o.transition,
            actor: { id: actor },
            object_id: objectId,
            inputs,
            ...(propose ? { propose: true } : {}),
          },
      `${propose ? "propose " : ""}${recording ? `${o.creates}.record on` : `${o.transition} on`} ${objectId}`,
    );
    onDone(v);
  }

  if (error) return <p className="text-sm text-red-700">{error}</p>;
  const key = (o: TransitionOffer) => `${o.transition}/${o.creates ?? ""}`;
  const proposable = (o: TransitionOffer) =>
    o.creates === null &&
    decl?.transitions?.[o.transition]?.proposable === true;
  const start = (o: TransitionOffer, propose: boolean) =>
    Object.keys(o.inputs).length === 0
      ? void take(o, {}, propose)
      : setOpen(
          open?.key === key(o) && open.propose === propose
            ? null
            : { key: key(o), propose },
        );
  return (
    <ul className="space-y-1">
      {offers.map((o) => (
        <li key={key(o)} className="rounded border border-slate-200 p-2">
          <div className="flex items-center gap-2 text-sm">
            <span className={`rounded px-1.5 text-xs ${BADGE[o.availability]}`}>
              {o.availability.replace(/_/g, " ")}
            </span>
            <span className="font-mono">
              {o.creates ? `record ${o.creates}` : o.transition}
            </span>
            <span className="ml-auto flex gap-1">
              {o.availability !== "unavailable" && (
                <button className="btn-quiet" onClick={() => start(o, false)}>
                  Take
                </button>
              )}
              {proposable(o) && (
                <button className="btn-quiet" onClick={() => start(o, true)}>
                  Propose
                </button>
              )}
            </span>
          </div>
          {o.verdict && (
            <div className="mt-1 text-xs text-amber-900">
              {explain(o.verdict)}
              {"remedy" in o.verdict && o.verdict.remedy && (
                <span className="text-slate-500"> · {o.verdict.remedy}</span>
              )}
            </div>
          )}
          {o.unevaluated.length > 0 && (
            <div className="mt-1 text-xs text-violet-800">
              not evaluated, an evaluator would be asked:{" "}
              {o.unevaluated.join(", ")}
            </div>
          )}
          {o.flags.length > 0 && (
            <div className="mt-1 text-xs text-sky-900">
              would warn: {o.flags.map((f) => f.clause).join(", ")}
            </div>
          )}
          {open?.key === key(o) && (
            <InputForm
              schema={o.inputs}
              enumerations={decl?.enumerations ?? {}}
              known={known}
              action={open.propose ? "Propose" : "Request"}
              onSubmit={(inputs) => void take(o, inputs, open.propose)}
              onCancel={() => setOpen(null)}
            />
          )}
        </li>
      ))}
    </ul>
  );
}
