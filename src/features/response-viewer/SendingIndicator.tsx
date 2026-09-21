// http_client/src/features/response-viewer/SendingIndicator.tsx
//
// Replaces the plain "Sending…" text in ResponseViewer while a request is
// in flight. Colors are theme tokens (src/index.css / tailwind.config.ts —
// primary is nord8, frost-teal/frost-deep are nord7/nord10), and the two
// custom animations (nord-reverse-orbit, nord-pulse-glow) live in
// tailwind.config.ts alongside Tailwind's built-in `animate-spin`. Uses
// lucide-react's Send icon rather than the Font Awesome paper-plane from
// the reference markup — the project has no Font Awesome dependency and
// Send is already the icon on the request builder's own Send button.
import { Send } from "lucide-react";

export function SendingIndicator() {
  return (
    <div
      aria-label="Sending request"
      className="flex h-32 w-32 items-center justify-center rounded-2xl border border-border/60 bg-card shadow-inner"
      role="status"
    >
      <div className="relative flex h-20 w-20 items-center justify-center">
        {/* Outer spinning gradient ring */}
        <div className="absolute inset-0 animate-spin rounded-full border-4 border-l-transparent border-t-primary border-r-frost-teal border-b-frost-deep" />
        {/* Inner reversing dashed ring */}
        <div className="absolute inset-2 animate-nord-reverse-orbit rounded-full border-2 border-dashed border-foreground/60" />
        {/* Center icon */}
        <Send aria-hidden className="h-5 w-5 animate-nord-pulse-glow text-primary" />
      </div>
    </div>
  );
}
