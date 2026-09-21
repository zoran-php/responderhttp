// http_client/src/services/environments.ts
//
// environment commands. The boundary crossing lives in services/invoke.ts
// (CLAUDE.md section 11, rule 3). One function per command in
// src-tauri/src/commands/environments.rs.
import { call } from "@/services/invoke";
import type {
  Environment,
  EnvironmentVariable,
  EnvironmentVariableInput,
} from "@/types/environments";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

export function listEnvironments(): Promise<Result<Environment[], ApiError>> {
  return call<Environment[]>("list_environments");
}

export function createEnvironment(name: string): Promise<Result<Environment, ApiError>> {
  return call<Environment>("create_environment", { name });
}

export function renameEnvironment(id: string, name: string): Promise<Result<void, ApiError>> {
  return call<void>("rename_environment", { id, name });
}

export function deleteEnvironment(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_environment", { id });
}

export function environmentVariables(
  environmentId: string,
): Promise<Result<EnvironmentVariable[], ApiError>> {
  return call<EnvironmentVariable[]>("environment_variables", { environmentId });
}

export function setEnvironmentVariables(
  environmentId: string,
  variables: EnvironmentVariableInput[],
): Promise<Result<void, ApiError>> {
  return call<void>("set_environment_variables", { environmentId, variables });
}
