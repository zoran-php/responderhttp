// http_client/src/lib/openapi-import.ts
//
// Pure helpers for the OpenAPI import dialog, kept out of the component
// (CLAUDE.md section 6).
import type {
  EnvironmentPreview,
  GroupingPreview,
  ImportGrouping,
  OpenApiImportOptions,
  OpenApiImportPreview,
  OpenApiImportRefusal,
} from "@/types/openapi-import";

/** How many top-level folder names the dialog lists before summarising. */
export const MAX_LISTED_FOLDERS = 12;

/** The options a freshly loaded file opens with. */
export function defaultImportOptions(preview: OpenApiImportPreview): OpenApiImportOptions {
  return {
    grouping: preview.defaultGrouping,
    includeExamples: true,
    createEnvironment: true,
  };
}

export function groupingPreview(
  preview: OpenApiImportPreview,
  grouping: ImportGrouping,
): GroupingPreview | undefined {
  return preview.groupings.find((candidate) => candidate.grouping === grouping);
}

/** "3 folders: pets, store, user" or "49 folders: a, b, … and 37 more". */
export function folderSummary(grouping: GroupingPreview | undefined): string {
  if (!grouping || grouping.folderCount === 0) {
    return "No folders - every request goes straight into the collection.";
  }
  const noun = grouping.folderCount === 1 ? "folder" : "folders";
  const listed = grouping.topLevel.slice(0, MAX_LISTED_FOLDERS);
  // Nested folders count towards the total but are not listed by name.
  const unlisted = grouping.topLevelCount - listed.length;
  const nested = grouping.folderCount - grouping.topLevelCount;
  const extras: string[] = [];
  if (unlisted > 0) {
    extras.push(`${unlisted} more`);
  }
  if (nested > 0) {
    extras.push(`${nested} nested`);
  }
  const suffix = extras.length > 0 ? `, and ${extras.join(" and ")}` : "";
  return `${grouping.folderCount} ${noun}: ${listed.join(", ")}${suffix}`;
}

/** "12 requests" / "1 request". */
export function countLabel(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

/** Environments this small show their variables without a "Show all". */
export const ALWAYS_LISTED_VARIABLES = 6;
/** How many secret names the summary line spells out. */
export const MAX_NAMED_SECRETS = 5;

/**
 * The one-line description of the environment an import would create:
 * "162 variables — baseUrl, 2 secret (key, token), 159 others".
 *
 * The first variable is the base URL whenever an environment is created, so
 * it is named; secrets are named because they are what the user has to fill
 * in; everything else is only counted.
 */
export function environmentSummary(environment: EnvironmentPreview): string {
  const [first, ...rest] = environment.variables;
  if (first === undefined) {
    return "no variables";
  }
  const base = first.secret ? null : first.name;
  const secrets = (base === null ? environment.variables : rest).filter(
    (variable) => variable.secret,
  );
  const others = environment.variables.length - secrets.length - (base === null ? 0 : 1);

  const parts: string[] = [];
  if (base !== null) {
    parts.push(base);
  }
  if (secrets.length > 0) {
    const named = secrets.slice(0, MAX_NAMED_SECRETS).map((variable) => variable.name);
    const unnamed = secrets.length - named.length;
    const list = unnamed > 0 ? `${named.join(", ")} and ${unnamed} more` : named.join(", ");
    parts.push(`${secrets.length} secret (${list})`);
  }
  if (others > 0) {
    parts.push(countLabel(others, "other", "others"));
  }
  const total = countLabel(environment.variables.length, "variable", "variables");
  return `${total} - ${parts.join(", ")}`;
}

/** Plain text for the clipboard, so a refusal can be pasted into an issue. */
export function refusalReport(refusal: OpenApiImportRefusal): string {
  const lines = [`${refusal.fileName}: ${refusal.title}`, refusal.reason];
  if (refusal.details.length > 0) {
    lines.push("", ...refusal.details.map((detail) => `- ${detail}`));
  }
  if (refusal.totalDetails > refusal.details.length) {
    lines.push(`(${refusal.totalDetails - refusal.details.length} more not shown)`);
  }
  return lines.join("\n");
}
