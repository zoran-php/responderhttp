// http_client/src/components/LazyCodeEditor.tsx
//
// Monaco is several megabytes, so it loads on first use rather than at
// startup (CLAUDE.md section 6).
import { lazy, Suspense } from "react";

const CodeEditor = lazy(() => import("@/components/CodeEditor"));

interface LazyCodeEditorProps {
  value: string;
  language: string;
  readOnly?: boolean;
  onChange?: (value: string) => void;
}

export function LazyCodeEditor(props: LazyCodeEditorProps) {
  return (
    <Suspense
      fallback={
        <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
          Loading editor…
        </div>
      }
    >
      <CodeEditor {...props} />
    </Suspense>
  );
}
