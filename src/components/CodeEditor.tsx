// http_client/src/components/CodeEditor.tsx
//
// The single Monaco wrapper. Everything that shows code goes through it, so
// the editor is configured once. Import it lazily (see LazyCodeEditor).
import Editor from "@monaco-editor/react";

import "@/lib/monaco";

interface CodeEditorProps {
  value: string;
  language: string;
  readOnly?: boolean;
  onChange?: (value: string) => void;
}

export default function CodeEditor({ value, language, readOnly, onChange }: CodeEditorProps) {
  return (
    <Editor
      value={value}
      language={language}
      theme="vs-dark"
      onChange={(next) => onChange?.(next ?? "")}
      options={{
        readOnly,
        domReadOnly: readOnly,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        wordWrap: "on",
        fontSize: 13,
        automaticLayout: true,
        tabSize: 2,
        renderLineHighlight: readOnly ? "none" : "line",
      }}
    />
  );
}
