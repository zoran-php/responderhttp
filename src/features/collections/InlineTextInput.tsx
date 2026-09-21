// http_client/src/features/collections/InlineTextInput.tsx
//
// Rename and "new folder/collection" both need the same thing: a row that
// turns into a text field, commits on Enter or blur, and cancels on Escape.
import { useRef, useState } from "react";

interface InlineTextInputProps {
  initialValue?: string;
  placeholder?: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
}

export function InlineTextInput({
  initialValue = "",
  placeholder,
  onCommit,
  onCancel,
}: InlineTextInputProps) {
  const [value, setValue] = useState(initialValue);
  // Enter commits and the parent then unmounts this input on its next
  // render, but guards against a stray blur firing a second commit first.
  const settledRef = useRef(false);

  function commit() {
    if (settledRef.current) {
      return;
    }
    settledRef.current = true;
    const trimmed = value.trim();
    if (trimmed === "") {
      onCancel();
      return;
    }
    onCommit(trimmed);
  }

  function cancel() {
    if (settledRef.current) {
      return;
    }
    settledRef.current = true;
    onCancel();
  }

  return (
    <input
      autoFocus
      className="h-6 w-full rounded border border-input bg-background px-1 text-sm"
      onBlur={commit}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          commit();
        } else if (event.key === "Escape") {
          event.preventDefault();
          cancel();
        }
      }}
      placeholder={placeholder}
      spellCheck={false}
      value={value}
    />
  );
}
