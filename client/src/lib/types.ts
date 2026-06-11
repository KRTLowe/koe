export interface AppConfig {
  serverUrl: string;
  clientId: string;
  passkey: string;
  acpUrl?: string | null;
  storagePath?: string | null;
  acpCwd?: string | null;
  floatImage?: string | null;
  allowedReadPaths?: string[];
  allowedWritePaths?: string[];
  deniedExtensions?: string[];
  toolPermissions?: Record<string, boolean>;
}

export interface TransferRecord {
  id: string;
  name: string;
  size: number;
  direction: "received" | "sent";
  timestamp: number;
  status: "ok" | "error";
  path?: string;
}
