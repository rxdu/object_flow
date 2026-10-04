import mermaid from "mermaid";
import { useEffect, useId, useState } from "react";

mermaid.initialize({
  startOnLoad: false,
  securityLevel: "strict",
  theme: "neutral",
});

/** A type's lifecycle, drawn by Mermaid from the source the documents'
 * diagrams are drawn from (scripts/flow-diagram.py). */
export function Diagram({ source }: { source: string }) {
  const id = useId().replace(/:/g, "");
  const [svg, setSvg] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    mermaid
      .render(`diagram-${id}`, source)
      .then(({ svg }) => {
        if (live) {
          setSvg(svg);
          setError(null);
        }
      })
      .catch((e: unknown) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [id, source]);

  if (error)
    return (
      <pre className="text-sm text-red-700 whitespace-pre-wrap">{error}</pre>
    );
  // Mermaid's own output at its strict security level, drawn from the module
  return (
    <div
      className="overflow-x-auto [&_svg]:max-w-none"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
