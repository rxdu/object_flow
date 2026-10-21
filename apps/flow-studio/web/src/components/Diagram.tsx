import mermaid from "mermaid";
import { useEffect, useId, useRef, useState } from "react";

mermaid.initialize({
  startOnLoad: false,
  securityLevel: "strict",
  theme: "neutral",
  themeVariables: {
    fontFamily: "ui-sans-serif, system-ui, sans-serif",
    fontSize: "15px",
  },
  // drawn at its own size rather than shrunk until its labels cannot be read
  state: { useMaxWidth: false },
  flowchart: { useMaxWidth: false },
});

/** A type's lifecycle, drawn by Mermaid from the source the documents'
 * diagrams are drawn from (scripts/flow-diagram.py): left to right where it
 * fits its card, else top to bottom, which a column holds, the source left as
 * the documents have it. */
export function Diagram({ source }: { source: string }) {
  const id = useId().replace(/:/g, "");
  const box = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      let { svg } = await mermaid.render(`diagram-${id}`, source);
      const wide = Number(/width="([\d.]+)"/.exec(svg)?.[1] ?? 0);
      const room = box.current?.clientWidth ?? Infinity;
      if (wide > room && source.includes("direction LR")) {
        ({ svg } = await mermaid.render(
          `diagram-${id}`,
          source.replace("direction LR", "direction TB"),
        ));
      }
      if (live) {
        setSvg(svg);
        setError(null);
      }
    })().catch((e: unknown) => {
      if (live) setError(String(e));
    });
    return () => {
      live = false;
    };
  }, [id, source]);

  if (error)
    return (
      <pre className="text-sm whitespace-pre-wrap text-red-700">{error}</pre>
    );
  // Mermaid's own output at its strict security level, drawn from the module
  return (
    <div
      ref={box}
      className="overflow-x-auto py-2 [&_svg]:mx-auto [&_svg]:max-w-none [&_svg]:shrink-0"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
