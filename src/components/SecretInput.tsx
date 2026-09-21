// http_client/src/components/SecretInput.tsx
//
// A text input that is masked until the user asks to see it. Used for every
// value the app treats as a secret (PLAN.md Phase 9): the Auth tab's secret
// fields and secret environment variables. Whether it is revealed is
// ephemeral UI state, so it lives here rather than in a store.
import { useState } from "react";
import { Eye, EyeOff } from "lucide-react";

import { cn } from "@/lib/utils";

interface SecretInputProps {
  value: string;
  onChange: (value: string) => void;
  /** Applied to the input itself. */
  className?: string;
  "aria-label"?: string;
  placeholder?: string;
}

export function SecretInput({
  value,
  onChange,
  className,
  "aria-label": ariaLabel,
  placeholder,
}: SecretInputProps) {
  const [revealed, setRevealed] = useState(false);
  const toggleLabel = revealed ? "Hide value" : "Show value";

  return (
    <div className="relative flex w-full items-center">
      <input
        aria-label={ariaLabel}
        autoComplete="off"
        className={cn(className, "pr-8")}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        spellCheck={false}
        type={revealed ? "text" : "password"}
        value={value}
      />
      <button
        aria-label={toggleLabel}
        aria-pressed={revealed}
        className="absolute right-1 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        onClick={() => setRevealed((current) => !current)}
        title={toggleLabel}
        type="button"
      >
        {revealed ? (
          <EyeOff aria-hidden className="h-3.5 w-3.5" />
        ) : (
          <Eye aria-hidden className="h-3.5 w-3.5" />
        )}
      </button>
    </div>
  );
}
