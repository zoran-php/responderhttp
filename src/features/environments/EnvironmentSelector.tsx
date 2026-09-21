// http_client/src/features/environments/EnvironmentSelector.tsx
//
// Picks which environment requests resolve against. Separate from whatever
// is open for editing on purpose: the one you are sending to should be
// visible at all times, not inferred from the sidebar selection.
import { useEnvironmentsStore } from "@/store/environments-store";

const NO_ENVIRONMENT = "";

export function EnvironmentSelector() {
  const environments = useEnvironmentsStore((state) => state.environments);
  const activeEnvironmentId = useEnvironmentsStore((state) => state.activeEnvironmentId);
  const setActiveEnvironment = useEnvironmentsStore((state) => state.setActiveEnvironment);

  return (
    <select
      aria-label="Active environment"
      className="h-7 max-w-44 shrink-0 rounded-md border border-input bg-background px-2 text-xs"
      onChange={(event) =>
        setActiveEnvironment(event.target.value === NO_ENVIRONMENT ? null : event.target.value)
      }
      value={activeEnvironmentId ?? NO_ENVIRONMENT}
    >
      <option value={NO_ENVIRONMENT}>No environment</option>
      {environments.map((environment) => (
        <option key={environment.id} value={environment.id}>
          {environment.name}
        </option>
      ))}
    </select>
  );
}
