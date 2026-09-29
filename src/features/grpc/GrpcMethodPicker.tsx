// http_client/src/features/grpc/GrpcMethodPicker.tsx
//
// The method list: every method of the loaded schema, grouped by service,
// each with its kind. Opening it is one of the two moments reflection runs
// (assumption 6), which the parent handles through `onOpen`.
import { findMethod } from "@/lib/grpc-request";
import { methodKindLabel } from "@/lib/grpc-status";
import type { ProtoSchema } from "@/types/grpc";

interface GrpcMethodPickerProps {
  schema: ProtoSchema | null;
  loading: boolean;
  methodPath: string;
  onChange: (methodPath: string) => void;
  onOpen: () => void;
}

export function GrpcMethodPicker({
  schema,
  loading,
  methodPath,
  onChange,
  onOpen,
}: GrpcMethodPickerProps) {
  // A saved request can name a method its schema no longer has; it stays
  // selected, marked, rather than silently becoming "no method".
  const missing = methodPath !== "" && schema !== null && findMethod(schema, methodPath) === null;
  const placeholder = loading
    ? "Loading methods…"
    : schema === null
      ? "No schema loaded"
      : "Select a method";

  return (
    <select
      aria-label="Method"
      className="h-9 w-72 shrink-0 rounded-md border border-input bg-background px-2 text-sm"
      onChange={(event) => onChange(event.target.value)}
      onFocus={onOpen}
      value={methodPath}
    >
      <option value="">{placeholder}</option>
      {missing && <option value={methodPath}>{`${methodPath} (not in the schema)`}</option>}
      {schema?.services.map((service) => (
        <optgroup key={service.name} label={service.name}>
          {service.methods.map((method) => (
            <option key={method.path} value={method.path}>
              {`${method.name} · ${methodKindLabel(method.kind)}`}
            </option>
          ))}
        </optgroup>
      ))}
    </select>
  );
}
