// http_client/src/lib/monaco.ts
//
// Points @monaco-editor/react at the bundled copy of Monaco. Without this it
// fetches the editor from a CDN at runtime, which fails offline and breaks
// the no-external-dependency rule (CLAUDE.md section 1).
//
// Imported for its side effects by components/CodeEditor.tsx, which is itself
// lazily loaded, so Monaco stays out of the initial bundle.
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";

declare global {
  interface Window {
    MonacoEnvironment?: monaco.Environment;
  }
}

// Only the languages we actually show get a worker; XML highlights without
// one, and pulling in the TypeScript/CSS workers would cost megabytes.
window.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    if (label === "json") {
      return new jsonWorker();
    }
    if (label === "html" || label === "handlebars" || label === "razor") {
      return new htmlWorker();
    }
    return new editorWorker();
  },
};

loader.config({ monaco });
