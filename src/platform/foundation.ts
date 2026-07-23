import { invoke } from "@tauri-apps/api/core";

export interface FoundationStatus {
  appVersion: string;
  databaseSchemaVersion: number;
  settingsSchemaVersion: number;
  runtimeState: "running" | "stopped";
}

export async function getFoundationStatus(): Promise<FoundationStatus> {
  return invoke<FoundationStatus>("foundation_status");
}
