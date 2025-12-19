import { useState } from "react";
import type { Event } from "nostr-tools";

interface EventDisplayProps {
  event: Event;
  title?: string;
}

export function EventDisplay({ event, title = "DVM Request Event" }: EventDisplayProps) {
  const [expanded, setExpanded] = useState(false);

  const formattedJson = JSON.stringify(event, null, 2);

  return (
    <div className="event-display">
      <button
        className="event-toggle"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        {expanded ? "▼" : "▶"} {title}
      </button>
      {expanded && (
        <pre className="event-json">
          <code>{formattedJson}</code>
        </pre>
      )}
    </div>
  );
}
